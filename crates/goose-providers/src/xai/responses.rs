//! xAI Responses API adapter.
//! Provides payload rewriting and streaming for xAI's Responses-compatible endpoint.
//! Matches the spec from packages/ai/src/api/xai-responses.ts in pi.

use super::shared::{
    grok_supports_client_side_tools, grok_supports_reasoning_effort, is_grok_cli_proxy_model,
    is_xai_multi_agent_model, normalized_xai_model_id, xai_base_url_for_model,
    xai_model_request_headers,
};
use crate::conversation::message::Message;
use crate::errors::ProviderError;
use crate::formats::openai_responses::create_responses_request;
use crate::model::ModelConfig;
use crate::openai_compatible::stream_responses_compat;
use anyhow::Result;
use reqwest::Client;
use rmcp::model::Tool;
use serde_json::Value;
use std::collections::HashMap;

/// Options for xAI Responses streaming.
#[derive(Debug, Clone, Default)]
pub struct XaiResponsesStreamOptions {
    pub session_id: Option<String>,
    pub reasoning_effort: Option<String>,
    pub extra_headers: Option<HashMap<String, String>>,
    /// Optional pre-computed Authorization header value (e.g. "Bearer xxx")
    pub authorization: Option<String>,
}

/// Rewrite a Responses payload for xAI compatibility.
/// Core logic matching pi's rewriteXaiResponsesPayload.
pub fn rewrite_xai_responses_payload(
    mut payload: Value,
    model_id: &str,
    options: &XaiResponsesStreamOptions,
) -> Value {
    if !payload.is_object() {
        return payload;
    }
    let obj = payload.as_object_mut().unwrap();

    let norm = normalized_xai_model_id(model_id);
    let uses_proxy = is_grok_cli_proxy_model(&norm);
    let is_multi = is_xai_multi_agent_model(&norm);

    // Multi-agent models reject client tools without beta access
    if !grok_supports_client_side_tools(&norm) {
        obj.remove("tools");
        obj.remove("tool_choice");
        obj.remove("parallel_tool_calls");
    }

    // Process input array: move system/developer to instructions, drop reasoning for proxy models
    let mut new_input: Option<Vec<Value>> = None;
    let mut instructions: Vec<String> = Vec::new();

    if let Some(input_val) = obj.get("input") {
        if let Some(arr) = input_val.as_array() {
            let mut ni = Vec::new();
            let drop_reasoning = uses_proxy || is_multi;

            for item in arr.iter() {
                if !item.is_object() {
                    ni.push(item.clone());
                    continue;
                }
                let o = item.as_object().unwrap();

                if drop_reasoning && o.get("type").and_then(|v| v.as_str()) == Some("reasoning") {
                    continue;
                }
                if o.get("content")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| s.is_empty())
                {
                    continue;
                }

                let role = o.get("role").and_then(|v| v.as_str());
                if role == Some("developer") || role == Some("system") {
                    if let Some(c) = o.get("content") {
                        if let Some(t) = c.as_str() {
                            if !t.trim().is_empty() {
                                instructions.push(t.trim().to_string());
                            }
                        }
                    }
                    continue;
                }
                ni.push(item.clone());
            }
            new_input = Some(ni);
        }
    }

    if !instructions.is_empty() {
        let existing = obj
            .get("instructions")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let combined = if existing.is_empty() {
            instructions.join("\n\n")
        } else {
            format!("{}\n\n{}", existing, instructions.join("\n\n"))
        };
        obj.insert("instructions".to_string(), Value::String(combined));
    }

    if let Some(ni) = new_input {
        obj.insert("input".to_string(), Value::Array(ni));
    }

    // response_format -> text.format
    if obj.contains_key("response_format") && !obj.contains_key("text") {
        if let Some(fmt) = obj.remove("response_format") {
            let mut text = serde_json::Map::new();
            text.insert("format".to_string(), fmt);
            obj.insert("text".to_string(), Value::Object(text));
        }
    }

    // Reasoning effort mapping and gating
    if let Some(reasoning) = obj.get_mut("reasoning") {
        if let Some(r) = reasoning.as_object_mut() {
            if let Some(eff) = r.get("effort").and_then(|v| v.as_str()) {
                if eff != "none" && grok_supports_reasoning_effort(&norm) {
                    let mapped = if eff == "minimal" { "low" } else { eff };
                    r.insert("effort".to_string(), Value::String(mapped.to_string()));
                } else {
                    obj.remove("reasoning");
                }
            }
        }
    }

    // Strip encrypted reasoning for proxy models
    if uses_proxy || is_multi {
        if let Some(include) = obj.get_mut("include") {
            if let Some(inc_arr) = include.as_array_mut() {
                inc_arr.retain(|v| v.as_str() != Some("reasoning.encrypted_content"));
                if inc_arr.is_empty() {
                    obj.remove("include");
                }
            }
        }
    }

    obj.remove("prompt_cache_retention");

    if let Some(sid) = &options.session_id {
        if !obj.contains_key("prompt_cache_key") {
            obj.insert("prompt_cache_key".to_string(), Value::String(sid.clone()));
        }
    }

    payload
}

/// Create a Responses API request payload, apply xAI rewrite, and return (url, headers, body).
pub fn prepare_xai_responses_request(
    model: &ModelConfig,
    system: &str,
    messages: &[Message],
    tools: &[Tool],
    options: &XaiResponsesStreamOptions,
) -> Result<(String, HashMap<String, String>, Value), ProviderError> {
    let base_url = xai_base_url_for_model(&model.model_name);
    let url = format!("{}/responses", base_url.trim_end_matches('/'));

    // Build the base Responses payload using the existing helper
    let mut payload = create_responses_request(model, system, messages, tools)
        .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;

    // Apply xAI-specific rewriting
    payload = rewrite_xai_responses_payload(payload, &model.model_name, options);

    // Build headers
    let mut headers = HashMap::new();
    headers.insert("Content-Type".to_string(), "application/json".to_string());
    headers.insert("Accept".to_string(), "text/event-stream".to_string());

    // Inject model-specific headers (CLI proxy, etc.)
    let proxy_headers = xai_model_request_headers(&model.model_name, options.session_id.as_deref());
    for (k, v) in proxy_headers {
        headers.insert(k, v);
    }

    // Merge any extra headers from options
    if let Some(extra) = &options.extra_headers {
        for (k, v) in extra {
            headers.insert(k.clone(), v.clone());
        }
    }

    // Authorization (Bearer token) if provided
    if let Some(auth) = &options.authorization {
        headers.insert("Authorization".to_string(), auth.clone());
    }

    Ok((url, headers, payload))
}

/// Stream xAI Responses with rewriting and routing.
/// This is the main entry point matching pi's xai-responses.ts stream function.
pub async fn stream_xai_responses(
    model: ModelConfig,
    system: &str,
    messages: Vec<Message>,
    tools: Vec<Tool>,
    options: XaiResponsesStreamOptions,
) -> Result<crate::base::MessageStream, ProviderError> {
    let (url, headers, payload) =
        prepare_xai_responses_request(&model, system, &messages, &tools, &options)?;

    // Create HTTP client
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;

    // Build request
    let mut req = client.post(&url);
    for (k, v) in &headers {
        req = req.header(k, v);
    }

    // Add auth header if present in model config (passed via request_params or separate mechanism)
    // For now, assume the caller has set up auth via the ApiClient or the payload includes it.
    // In practice, XaiOAuthProvider will inject the Bearer token via its AuthProvider.

    let resp = req
        .json(&payload)
        .send()
        .await
        .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(ProviderError::RequestFailed(format!(
            "xAI Responses error {}: {}",
            status, text
        )));
    }

    // Use the existing Responses streaming parser
    let message_stream = stream_responses_compat(resp, None)?;

    // Adapt the (Message, Option<ProviderUsage>) stream to the expected return type
    // Note: stream_responses_compat returns Pin<Box<dyn Stream<Item = Result<(Message, Option<ProviderUsage>), ProviderError>>>>
    // We return it directly as it already matches the expected signature.
    Ok(message_stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_rewrite_system_to_instructions() {
        let p = json!({"model":"grok-4.3","input":[{"role":"system","content":"sys"},{"role":"user","content":"hi"}]});
        let r = rewrite_xai_responses_payload(p, "grok-4.3", &XaiResponsesStreamOptions::default());
        assert_eq!(r["instructions"], "sys");
        assert_eq!(r["input"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_rewrite_drops_tools_multi_agent() {
        let p = json!({"model":"grok-4.20-multi-agent-0309","tools":[1],"input":[]});
        let r = rewrite_xai_responses_payload(
            p,
            "grok-4.20-multi-agent-0309",
            &XaiResponsesStreamOptions::default(),
        );
        assert!(r.get("tools").is_none());
    }

    #[test]
    fn test_rewrite_effort_mapping() {
        let p = json!({"model":"grok-4.3","reasoning":{"effort":"minimal"},"input":[]});
        let r = rewrite_xai_responses_payload(p, "grok-4.3", &XaiResponsesStreamOptions::default());
        assert_eq!(r["reasoning"]["effort"], "low");
    }

    #[test]
    fn test_prepare_request_cli_proxy() {
        let model = ModelConfig {
            model_name: "grok-build".to_string(),
            context_limit: None,
            temperature: None,
            max_tokens: None,
            toolshim: false,
            toolshim_model: None,
            fast_model_config: None,
            request_params: None,
            reasoning: None,
        };
        let (url, headers, _payload) = prepare_xai_responses_request(
            &model,
            "",
            &[],
            &[],
            &XaiResponsesStreamOptions::default(),
        )
        .unwrap();
        assert!(url.contains("cli-chat-proxy.grok.com"));
        assert_eq!(headers.get("x-grok-client-identifier").unwrap(), "goose");
    }

    // === Expanded test suite mirroring pi's xai-responses-payload.test.ts ===

    #[test]
    fn test_rewrite_moves_developer_messages() {
        let p = json!({
            "model": "grok-4.3",
            "input": [
                {"role": "developer", "content": "be concise"},
                {"role": "user", "content": "hi"}
            ]
        });
        let r = rewrite_xai_responses_payload(p, "grok-4.3", &XaiResponsesStreamOptions::default());
        assert_eq!(r["instructions"], "be concise");
    }

    #[test]
    fn test_rewrite_drops_reasoning_for_proxy_models() {
        let p = json!({
            "model": "grok-build",
            "input": [
                {"type": "reasoning", "content": "secret"},
                {"role": "user", "content": "hi"}
            ]
        });
        let r =
            rewrite_xai_responses_payload(p, "grok-build", &XaiResponsesStreamOptions::default());
        let input = r["input"].as_array().unwrap();
        assert_eq!(input.len(), 1);
        assert_eq!(input[0]["role"], "user");
    }

    #[test]
    fn test_rewrite_strips_encrypted_reasoning_include_for_proxy() {
        let p = json!({
            "model": "grok-build",
            "include": ["reasoning.encrypted_content", "other"],
            "input": []
        });
        let r =
            rewrite_xai_responses_payload(p, "grok-build", &XaiResponsesStreamOptions::default());
        let inc = r["include"].as_array().unwrap();
        assert_eq!(inc.len(), 1);
        assert_eq!(inc[0], "other");
    }

    #[test]
    fn test_rewrite_response_format_to_text() {
        let p = json!({
            "model": "grok-4.3",
            "response_format": {"type": "json_object"},
            "input": []
        });
        let r = rewrite_xai_responses_payload(p, "grok-4.3", &XaiResponsesStreamOptions::default());
        assert!(r.get("response_format").is_none());
        assert_eq!(r["text"]["format"]["type"], "json_object");
    }

    #[test]
    fn test_rewrite_reasoning_effort_only_for_supported_models() {
        let p1 =
            json!({"model":"grok-4.20-0309-reasoning","reasoning":{"effort":"high"},"input":[]});
        let r1 = rewrite_xai_responses_payload(
            p1,
            "grok-4.20-0309-reasoning",
            &XaiResponsesStreamOptions::default(),
        );
        assert!(r1.get("reasoning").is_none()); // auto-reasoning model rejects explicit effort

        let p2 = json!({"model":"grok-4.3","reasoning":{"effort":"high"},"input":[]});
        let r2 =
            rewrite_xai_responses_payload(p2, "grok-4.3", &XaiResponsesStreamOptions::default());
        assert_eq!(r2["reasoning"]["effort"], "high");
    }

    #[test]
    fn test_rewrite_multi_agent_strips_tools() {
        let p = json!({
            "model": "grok-4.20-multi-agent-0309",
            "tools": [{"type":"function"}],
            "tool_choice": "auto",
            "input": []
        });
        let r = rewrite_xai_responses_payload(
            p,
            "grok-4.20-multi-agent-0309",
            &XaiResponsesStreamOptions::default(),
        );
        assert!(r.get("tools").is_none());
        assert!(r.get("tool_choice").is_none());
    }

    #[test]
    fn test_rewrite_preserves_prompt_cache_key_from_session() {
        let opts = XaiResponsesStreamOptions {
            session_id: Some("sess-123".to_string()),
            ..Default::default()
        };
        let p = json!({"model":"grok-4.3","input":[]});
        let r = rewrite_xai_responses_payload(p, "grok-4.3", &opts);
        assert_eq!(r["prompt_cache_key"], "sess-123");
    }

    #[test]
    fn test_rewrite_drops_empty_content_items_for_proxy() {
        let p = json!({
            "model": "grok-build",
            "input": [
                {"role": "system", "content": ""},
                {"role": "user", "content": "hi"}
            ]
        });
        let r =
            rewrite_xai_responses_payload(p, "grok-build", &XaiResponsesStreamOptions::default());
        let input = r["input"].as_array().unwrap();
        assert_eq!(input.len(), 1);
    }

    #[test]
    fn test_rewrite_appends_to_existing_instructions() {
        let p = json!({
            "model": "grok-4.3",
            "instructions": "base",
            "input": [{"role": "developer", "content": "extra"}]
        });
        let r = rewrite_xai_responses_payload(p, "grok-4.3", &XaiResponsesStreamOptions::default());
        assert_eq!(r["instructions"], "base\n\nextra");
    }

    #[test]
    fn test_rewrite_non_object_payload_passthrough() {
        let p = json!("not an object");
        let r = rewrite_xai_responses_payload(
            p.clone(),
            "grok-4.3",
            &XaiResponsesStreamOptions::default(),
        );
        assert_eq!(r, p);
    }

    // Effort matrix test mirroring pi's xai-effort-matrix.mjs
    #[test]
    fn test_reasoning_effort_matrix() {
        let cases = vec![
            ("grok-4.3", "high", true),
            ("grok-4.20-multi-agent-0309", "medium", true),
            ("grok-4.20-0309-reasoning", "high", false),
            ("grok-build", "low", false),
            ("grok-build-0.1", "minimal", false),
        ];
        for (model, effort, should_keep) in cases {
            let p = json!({
                "model": model,
                "reasoning": {"effort": effort},
                "input": []
            });
            let r = rewrite_xai_responses_payload(p, model, &XaiResponsesStreamOptions::default());
            if should_keep {
                let mapped = if effort == "minimal" { "low" } else { effort };
                assert_eq!(r["reasoning"]["effort"], mapped);
            } else {
                assert!(r.get("reasoning").is_none() || r["reasoning"].get("effort").is_none());
            }
        }
    }
}
