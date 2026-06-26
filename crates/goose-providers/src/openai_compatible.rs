use crate::conversation::token_usage::ProviderUsage;
use crate::images::ImageFormat;
use anyhow::Error;
use async_stream::try_stream;
use futures::TryStreamExt;
use reqwest::Response;
#[cfg(test)]
use reqwest::StatusCode;
use serde_json::Value;
use tokio::pin;
use tokio_stream::StreamExt;
use tokio_util::codec::{FramedRead, LinesCodec};
use tokio_util::io::StreamReader;

use super::api_client::ApiClient;
use super::base::{stream_from_single_message, MessageStream, Provider};
use super::retry::ProviderRetry;
use crate::conversation::message::Message;
use crate::errors::ProviderError;
use crate::formats::openai::{
    create_request, get_usage, response_to_message, response_to_streaming_message,
};
use crate::formats::openai_responses::responses_api_to_streaming_message;
use crate::model::ModelConfig;
use crate::request_log::{start_log, LoggerHandleExt, RequestLogHandle};
use rmcp::model::Tool;

pub struct OpenAiCompatibleProvider {
    name: String,
    /// Client targeted at the base URL (e.g. `https://api.x.ai/v1`)
    api_client: ApiClient,
    model: ModelConfig,
    /// Path prefix prepended to `chat/completions` (e.g. `"deployments/{name}/"` for Azure).
    completions_prefix: String,
    supports_streaming: bool,
    /// When set, the model list and per-model context are sourced from the
    /// provider's own `/v1/models` (context_length / context_window) instead of
    /// the canonical registry alone — used by the xAI providers.
    rich_models: bool,
}

impl OpenAiCompatibleProvider {
    pub fn new(
        name: String,
        api_client: ApiClient,
        model: ModelConfig,
        completions_prefix: String,
    ) -> Self {
        Self {
            name,
            api_client,
            model,
            completions_prefix,
            supports_streaming: true,
            rich_models: false,
        }
    }

    pub fn with_supports_streaming(mut self, supports_streaming: bool) -> Self {
        self.supports_streaming = supports_streaming;
        self
    }

    /// Source the model list + per-model context from the provider's own
    /// `/v1/models` response (xAI). Off by default so other OpenAI-compatible
    /// providers are unaffected.
    pub fn with_rich_models(mut self, rich_models: bool) -> Self {
        self.rich_models = rich_models;
        self
    }

    /// Fetch and validate the `/v1/models` JSON body.
    async fn fetch_models_json(&self) -> Result<Value, ProviderError> {
        let response = self
            .api_client
            .response_get(None, "models")
            .await
            .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;
        let json = handle_response_openai_compat(response).await?;
        if let Some(err_obj) = json.get("error") {
            let msg = err_obj
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            return Err(ProviderError::Authentication(msg.to_string()));
        }
        Ok(json)
    }

    /// Live context window for `model_name` from the provider's `/v1/models`,
    /// or `None` if unavailable. Bounded by a short timeout so a slow endpoint
    /// can't stall provider construction.
    pub async fn api_context_limit(&self, model_name: &str) -> Option<usize> {
        const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
        let json = tokio::time::timeout(PROBE_TIMEOUT, self.fetch_models_json())
            .await
            .ok()?
            .ok()?;
        crate::openai::parse_n_ctx_from_models(&json, model_name)
    }

    /// If the model has no context limit yet, set it from the live `/v1/models`
    /// response, falling back to `fallback` (e.g. a curated offline map) when
    /// the API is unavailable. Builder-style for use in provider construction.
    pub async fn ensure_context_limit(mut self, fallback: Option<usize>) -> Self {
        if self.model.context_limit.is_none() {
            let model_name = self.model.model_name.clone();
            self.model.context_limit = self.api_context_limit(&model_name).await.or(fallback);
        }
        self
    }

    fn build_request(
        &self,
        model_config: &ModelConfig,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
        for_streaming: bool,
    ) -> Result<Value, ProviderError> {
        create_request(
            model_config,
            system,
            messages,
            tools,
            &ImageFormat::OpenAi,
            for_streaming,
        )
        .map_err(|e| ProviderError::RequestFailed(format!("Failed to create request: {}", e)))
    }
}

#[async_trait::async_trait]
impl Provider for OpenAiCompatibleProvider {
    fn get_name(&self) -> &str {
        &self.name
    }

    fn get_model_config(&self) -> ModelConfig {
        self.model.clone()
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        let json = self.fetch_models_json().await?;
        let arr = json.get("data").and_then(|v| v.as_array()).ok_or_else(|| {
            ProviderError::RequestFailed("Missing 'data' array in models response".to_string())
        })?;
        let mut models: Vec<String> = arr
            .iter()
            .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(str::to_string))
            .collect();
        models.sort();
        Ok(models)
    }

    /// When `rich_models` is set, source the catalog from the provider's own
    /// `/v1/models`: start from the canonical per-model metadata (so curated
    /// capability flags are preserved) and override the context limit with the
    /// live `context_length` / `context_window` reported by the API. Falls back
    /// to the trait default (canonical only) when the endpoint is unavailable.
    async fn fetch_supported_model_info(
        &self,
    ) -> Result<Vec<crate::base::ModelInfo>, ProviderError> {
        if !self.rich_models {
            let names = self.fetch_supported_models().await?;
            return Ok(names
                .iter()
                .map(|n| crate::base::model_info_for_provider_model(&self.name, n))
                .collect());
        }
        let json = self.fetch_models_json().await?;
        let data = json.get("data").and_then(|v| v.as_array()).ok_or_else(|| {
            ProviderError::RequestFailed("Missing 'data' array in models response".to_string())
        })?;
        let mut infos: Vec<crate::base::ModelInfo> = data
            .iter()
            .filter_map(|entry| {
                let id = entry.get("id").and_then(|v| v.as_str())?;
                let mut info = crate::base::model_info_for_provider_model(&self.name, id);
                // API-reported context (context_length / context_window) is the
                // source of truth; keep canonical caps otherwise.
                if let Some(rich) = crate::openai::rich_model_to_info(entry) {
                    info.context_limit = rich.context_limit;
                    if rich.supports_vision.is_some() {
                        info.supports_vision = rich.supports_vision;
                    }
                    if rich.supports_tools.is_some() {
                        info.supports_tools = rich.supports_tools;
                    }
                }
                Some(info)
            })
            .collect();
        infos.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(infos)
    }

    async fn stream(
        &self,
        model_config: &ModelConfig,
        session_id: &str,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        let payload = self.build_request(
            model_config,
            system,
            messages,
            tools,
            self.supports_streaming,
        )?;
        let mut log = start_log(model_config, &payload)?;

        let completions_path = format!("{}chat/completions", self.completions_prefix);
        let response = self
            .with_retry(|| async {
                let resp = self
                    .api_client
                    .response_post(Some(session_id), &completions_path, &payload)
                    .await?;
                handle_status(resp).await
            })
            .await
            .inspect_err(|e| {
                let _ = log.error(e);
            })?;

        if self.supports_streaming {
            stream_openai_compat(response, log)
        } else {
            let json: serde_json::Value = response.json().await.map_err(|e| {
                ProviderError::RequestFailed(format!("Failed to parse JSON: {}", e))
            })?;

            let message = response_to_message(&json).map_err(|e| {
                ProviderError::RequestFailed(format!("Failed to parse message: {}", e))
            })?;

            let usage_data = get_usage(json.get("usage").unwrap_or(&serde_json::Value::Null));
            let usage = ProviderUsage::new(model_config.model_name.clone(), usage_data);

            log.write(
                &serde_json::to_value(&message).unwrap_or_default(),
                Some(&usage.usage),
            )?;

            Ok(stream_from_single_message(message, usage))
        }
    }
}

// Re-exported from the dedicated `http_status` module — these helpers are
// format-agnostic and used across all provider families.
pub use super::http_status::{
    handle_response, handle_status, map_http_error_to_provider_error, sanitize_url,
};

// Legacy alias kept for callers that haven't migrated their import path yet.
pub use super::http_status::handle_response as handle_response_openai_compat;

pub fn stream_openai_compat(
    response: Response,
    mut log: Option<Box<dyn RequestLogHandle>>,
) -> Result<MessageStream, ProviderError> {
    let stream = response.bytes_stream().map_err(std::io::Error::other);

    Ok(Box::pin(try_stream! {
        let stream_reader = StreamReader::new(stream);
        let framed = FramedRead::new(stream_reader, LinesCodec::new())
            .map_err(Error::from);

        let message_stream = response_to_streaming_message(framed);
        pin!(message_stream);
        while let Some(message) = message_stream.next().await {
            let (message, usage) = message.map_err(|e|
                e.downcast::<ProviderError>()
                    .unwrap_or_else(ProviderError::stream_decode_error)
            )?;
            log.write(&message, usage.as_ref().map(|f| f.usage).as_ref())?;
            yield (message, usage);
        }
    }))
}

pub fn stream_responses_compat(
    response: Response,
    mut log: Option<Box<dyn RequestLogHandle>>,
) -> Result<MessageStream, ProviderError> {
    let stream = response.bytes_stream().map_err(std::io::Error::other);

    Ok(Box::pin(try_stream! {
        let stream_reader = StreamReader::new(stream);
        let framed = FramedRead::new(stream_reader, LinesCodec::new())
            .map_err(Error::from);

        let message_stream = responses_api_to_streaming_message(framed);
        pin!(message_stream);
        while let Some(message) = message_stream.next().await {
            let (message, usage) = message.map_err(|e|
                e.downcast::<ProviderError>()
                    .unwrap_or_else(ProviderError::stream_decode_error)
            )?;
            log.write(&message, usage.as_ref().map(|f| f.usage).as_ref())?;
            yield (message, usage);
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ModelConfig;
    use serde_json::json;
    use test_case::test_case;

    #[test_case(
        StatusCode::PAYMENT_REQUIRED,
        Some(json!({"error": {"message": "Insufficient credits to complete this request"}})),
        "CreditsExhausted"
        ; "402 with payload"
    )]
    #[test_case(
        StatusCode::PAYMENT_REQUIRED,
        None,
        "CreditsExhausted"
        ; "402 without payload"
    )]
    #[test_case(
        StatusCode::TOO_MANY_REQUESTS,
        Some(json!({"error": {"message": "Rate limit exceeded"}})),
        "RateLimitExceeded"
        ; "429 rate limit"
    )]
    #[test_case(
        StatusCode::UNAUTHORIZED,
        None,
        "Authentication"
        ; "401 unauthorized"
    )]
    #[test_case(
        StatusCode::BAD_REQUEST,
        Some(json!({"error": {"message": "This request exceeds the maximum context length"}})),
        "ContextLengthExceeded"
        ; "400 context length"
    )]
    #[test_case(
        StatusCode::INTERNAL_SERVER_ERROR,
        None,
        "ServerError"
        ; "500 server error"
    )]
    #[test_case(
        StatusCode::NOT_FOUND,
        None,
        "RequestFailed"
        ; "404 not found"
    )]
    #[test_case(
        StatusCode::NOT_FOUND,
        Some(json!({"error": {"message": "model not available"}})),
        "RequestFailed"
        ; "404 with error payload"
    )]
    fn http_status_maps_to_expected_error(
        status: StatusCode,
        payload: Option<Value>,
        expected_variant: &str,
    ) {
        let err = map_http_error_to_provider_error(status, payload, "http://test/endpoint");
        let actual = err.telemetry_type();
        let expected_telemetry = match expected_variant {
            "CreditsExhausted" => "credits_exhausted",
            "RateLimitExceeded" => "rate_limit",
            "Authentication" => "auth",
            "ContextLengthExceeded" => "context_length",
            "ServerError" => "server",
            "RequestFailed" => "request",
            other => panic!("Unknown variant: {other}"),
        };
        assert_eq!(
            actual, expected_telemetry,
            "Expected {expected_variant}, got error: {err:?}"
        );
    }

    #[test]
    fn build_request_respects_non_streaming_mode() {
        let provider = OpenAiCompatibleProvider::new(
            "test".to_string(),
            ApiClient::new_with_tls(
                "http://localhost".to_string(),
                super::super::api_client::AuthMethod::NoAuth,
                None,
            )
            .unwrap(),
            ModelConfig::new_or_fail("test-model"),
            String::new(),
        )
        .with_supports_streaming(false);

        let payload = provider
            .build_request(&provider.model, "", &[], &[], provider.supports_streaming)
            .unwrap();

        assert_eq!(payload.get("stream"), None);
        assert_eq!(payload.get("stream_options"), None);
    }
}
