use base64::Engine as _;
use lru::LruCache;
use rmcp::model::Tool;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use tiktoken_rs::CoreBPE;
use tokio::sync::OnceCell;

use crate::conversation::message::Message;

static TOKENIZER: OnceCell<Arc<CoreBPE>> = OnceCell::const_new();

const MAX_TOKEN_CACHE_SIZE: usize = 1_024;

// Image token estimation. Providers resize images server-side and bill by the
// resulting pixel area, so token cost tracks dimensions rather than the base64
// payload length. The divisor approximates Anthropic's (width*height)/750, the
// most expensive of the major providers at typical sizes, which keeps the
// estimate conservative for OpenAI/Gemini. The cap mirrors the ~1568px long-edge
// resize all of them apply. When dimensions can't be read we fall back to a
// non-zero estimate so an image is never counted as free.
const IMAGE_TOKEN_DIVISOR: f64 = 750.0;
const IMAGE_TOKEN_CAP: usize = 1_600;
const IMAGE_TOKEN_FALLBACK: usize = 1_000;
const IMAGE_HEADER_PREFIX_BYTES: usize = 64 * 1_024;

fn estimate_image_tokens(base64_data: &str) -> usize {
    let max_b64 = (IMAGE_HEADER_PREFIX_BYTES / 3 * 4).min(base64_data.len());
    let prefix_len = max_b64 - (max_b64 % 4);
    let Ok(bytes) = base64::prelude::BASE64_STANDARD.decode(&base64_data.as_bytes()[..prefix_len])
    else {
        return IMAGE_TOKEN_FALLBACK;
    };
    match image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()
        .and_then(|reader| reader.into_dimensions().ok())
    {
        Some((width, height)) => {
            let area = f64::from(width) * f64::from(height);
            ((area / IMAGE_TOKEN_DIVISOR).ceil() as usize).clamp(1, IMAGE_TOKEN_CAP)
        }
        None => IMAGE_TOKEN_FALLBACK,
    }
}

// token use for various bits of a tool calls:
const FUNC_INIT: usize = 7;
const PROP_INIT: usize = 3;
const PROP_KEY: usize = 3;
const ENUM_INIT: isize = -3;
const ENUM_ITEM: usize = 3;
const FUNC_END: usize = 12;

pub struct TokenCounter {
    tokenizer: Arc<CoreBPE>,
    token_cache: Mutex<LruCache<TokenCacheKey, usize>>,
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
struct TokenCacheKey {
    len: usize,
    hash: [u8; 32],
}

impl TokenCacheKey {
    fn from_text(text: &str) -> Self {
        Self {
            len: text.len(),
            hash: *blake3::hash(text.as_bytes()).as_bytes(),
        }
    }
}

impl TokenCounter {
    pub async fn new() -> Result<Self, String> {
        let tokenizer = get_tokenizer().await?;
        let cache_capacity =
            NonZeroUsize::new(MAX_TOKEN_CACHE_SIZE).expect("token cache capacity must be non-zero");
        Ok(Self {
            tokenizer,
            token_cache: Mutex::new(LruCache::new(cache_capacity)),
        })
    }

    pub fn count_tokens(&self, text: &str) -> usize {
        let cache_key = TokenCacheKey::from_text(text);
        if let Some(count) = self
            .token_cache
            .lock()
            .expect("token cache mutex poisoned")
            .get(&cache_key)
            .copied()
        {
            return count;
        }

        let tokens = self.tokenizer.encode_with_special_tokens(text);
        let count = tokens.len();

        self.token_cache
            .lock()
            .expect("token cache mutex poisoned")
            .put(cache_key, count);
        count
    }

    pub fn count_tokens_for_tools(&self, tools: &[Tool]) -> usize {
        let mut func_token_count = 0;
        if !tools.is_empty() {
            for tool in tools {
                func_token_count += FUNC_INIT;
                let name = &tool.name;
                let description = &tool
                    .description
                    .as_deref()
                    .unwrap_or_default()
                    .trim_end_matches('.');

                let line = format!("{}:{}", name, description);
                func_token_count += self.count_tokens(&line);

                if let Some(serde_json::Value::Object(properties)) =
                    tool.input_schema.get("properties")
                {
                    if !properties.is_empty() {
                        func_token_count += PROP_INIT;
                        for (key, value) in properties {
                            func_token_count += PROP_KEY;
                            let p_name = key;
                            let p_type = value.get("type").and_then(|v| v.as_str()).unwrap_or("");
                            let p_desc = value
                                .get("description")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .trim_end_matches('.');

                            let line = format!("{}:{}:{}", p_name, p_type, p_desc);
                            func_token_count += self.count_tokens(&line);

                            if let Some(enum_values) = value.get("enum").and_then(|v| v.as_array())
                            {
                                func_token_count =
                                    func_token_count.saturating_add_signed(ENUM_INIT);
                                for item in enum_values {
                                    if let Some(item_str) = item.as_str() {
                                        func_token_count += ENUM_ITEM;
                                        func_token_count += self.count_tokens(item_str);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            func_token_count += FUNC_END;
        }

        func_token_count
    }

    pub fn count_chat_tokens(
        &self,
        system_prompt: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> usize {
        let tokens_per_message = 4;
        let mut num_tokens = 0;

        if !system_prompt.is_empty() {
            num_tokens += self.count_tokens(system_prompt) + tokens_per_message;
        }

        for message in messages {
            if !message.metadata.agent_visible {
                continue;
            }
            num_tokens += tokens_per_message;
            for content in &message.content {
                if let Some(content_text) = content.as_text() {
                    num_tokens += self.count_tokens(content_text);
                } else if let Some(image) = content.as_image() {
                    num_tokens += estimate_image_tokens(&image.data);
                } else if let Some(tool_request) = content.as_tool_request() {
                    if let Ok(tool_call) = tool_request.tool_call.as_ref() {
                        let text = format!(
                            "{}:{}:{:?}",
                            tool_request.id, tool_call.name, tool_call.arguments
                        );
                        num_tokens += self.count_tokens(&text);
                    }
                } else if let Some(tool_response) = content.as_tool_response() {
                    if let Ok(result) = &tool_response.tool_result {
                        let texts: Vec<&str> = result
                            .content
                            .iter()
                            .filter_map(|p| p.as_text())
                            .map(|t| t.text.as_str())
                            .collect();
                        if !texts.is_empty() {
                            num_tokens += self.count_tokens(&texts.join("\n"));
                        }
                        for image in result.content.iter().filter_map(|p| p.as_image()) {
                            num_tokens += estimate_image_tokens(&image.data);
                        }
                    }
                }
            }
        }

        if !tools.is_empty() {
            num_tokens += self.count_tokens_for_tools(tools);
        }

        num_tokens += 3; // Reply primer

        num_tokens
    }

    pub fn count_everything(
        &self,
        system_prompt: &str,
        messages: &[Message],
        tools: &[Tool],
        resources: &[String],
    ) -> usize {
        let mut num_tokens = self.count_chat_tokens(system_prompt, messages, tools);

        if !resources.is_empty() {
            for resource in resources {
                num_tokens += self.count_tokens(resource);
            }
        }
        num_tokens
    }

    pub fn clear_cache(&self) {
        self.token_cache
            .lock()
            .expect("token cache mutex poisoned")
            .clear();
    }

    pub fn cache_size(&self) -> usize {
        self.token_cache
            .lock()
            .expect("token cache mutex poisoned")
            .len()
    }
}

async fn get_tokenizer() -> Result<Arc<CoreBPE>, String> {
    Ok(TOKENIZER
        .get_or_init(|| async {
            let bpe = tiktoken_rs::o200k_base().expect("Failed to initialize o200k_base tokenizer");
            Arc::new(bpe)
        })
        .await
        .clone())
}

pub async fn create_token_counter() -> Result<TokenCounter, String> {
    TokenCounter::new().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_caching() {
        let counter = create_token_counter().await.unwrap();

        let text = "This is a test for caching functionality";

        let count1 = counter.count_tokens(text);
        assert_eq!(counter.cache_size(), 1);

        let count2 = counter.count_tokens(text);
        assert_eq!(count1, count2);
        assert_eq!(counter.cache_size(), 1);

        let count3 = counter.count_tokens("Different text");
        assert_eq!(counter.cache_size(), 2);
        assert_ne!(count1, count3);
    }

    #[tokio::test]
    async fn test_cache_management() {
        let counter = create_token_counter().await.unwrap();

        counter.count_tokens("First text");
        counter.count_tokens("Second text");
        counter.count_tokens("Third text");

        assert_eq!(counter.cache_size(), 3);

        counter.clear_cache();
        assert_eq!(counter.cache_size(), 0);

        let count = counter.count_tokens("First text");
        assert!(count > 0);
        assert_eq!(counter.cache_size(), 1);
    }

    #[tokio::test]
    async fn test_concurrent_token_counter_creation() {
        let handles: Vec<_> = (0..10)
            .map(|_| tokio::spawn(async { create_token_counter().await.unwrap() }))
            .collect();

        let counters: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        let text = "Test concurrent creation";
        let expected_count = counters[0].count_tokens(text);

        for counter in &counters {
            assert_eq!(counter.count_tokens(text), expected_count);
        }
    }

    #[tokio::test]
    async fn test_cache_eviction_behavior() {
        let counter = create_token_counter().await.unwrap();

        let mut cached_texts = Vec::new();
        for i in 0..=MAX_TOKEN_CACHE_SIZE {
            let text = format!("Test string number {}", i);
            counter.count_tokens(&text);
            cached_texts.push(text);
        }

        assert_eq!(counter.cache_size(), MAX_TOKEN_CACHE_SIZE);

        let recent_text = &cached_texts[cached_texts.len() - 1];
        let start_size = counter.cache_size();

        counter.count_tokens(recent_text);
        assert_eq!(counter.cache_size(), start_size);
    }

    #[tokio::test]
    async fn test_concurrent_cache_operations() {
        let counter = std::sync::Arc::new(create_token_counter().await.unwrap());

        let handles: Vec<_> = (0..20)
            .map(|i| {
                let counter_clone = counter.clone();
                tokio::spawn(async move {
                    let text = format!("Concurrent test {}", i % 5);
                    counter_clone.count_tokens(&text)
                })
            })
            .collect();

        let results: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        for result in results {
            assert!(result > 0);
        }

        assert!(counter.cache_size() > 0);
        assert!(counter.cache_size() <= MAX_TOKEN_CACHE_SIZE);
    }

    fn png_base64(width: u32, height: u32) -> String {
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::new(width, height));
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
        base64::prelude::BASE64_STANDARD.encode(buf.into_inner())
    }

    #[tokio::test]
    async fn test_count_chat_tokens_counts_top_level_image() {
        use crate::conversation::message::Message;

        let counter = create_token_counter().await.unwrap();
        let text_only = Message::user().with_text("describe this");
        let with_image = Message::user()
            .with_text("describe this")
            .with_image(png_base64(300, 300), "image/png");

        let text_tokens = counter.count_chat_tokens("", std::slice::from_ref(&text_only), &[]);
        let image_tokens = counter.count_chat_tokens("", std::slice::from_ref(&with_image), &[]);

        assert_eq!(image_tokens - text_tokens, 120);
    }

    #[tokio::test]
    async fn test_count_chat_tokens_counts_image_in_tool_response() {
        use crate::conversation::message::Message;
        use rmcp::model::{CallToolResult, Content};

        let counter = create_token_counter().await.unwrap();
        let result =
            CallToolResult::success(vec![Content::image(png_base64(300, 300), "image/png")]);
        let message = Message::user().with_tool_response("call_1", Ok(result));

        let tokens = counter.count_chat_tokens("", std::slice::from_ref(&message), &[]);

        // tokens_per_message (4) + reply primer (3) + the 120-token image.
        assert_eq!(tokens, 4 + 3 + 120);
    }
}
