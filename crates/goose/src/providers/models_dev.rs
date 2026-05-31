use super::api_client::{ApiClient, AuthMethod};
use super::base::{ConfigKey, MessageStream, ModelInfo, Provider, ProviderDef, ProviderMetadata};
use super::errors::ProviderError;
use super::openai_compatible::{handle_status, stream_openai_compat};
use super::retry::ProviderRetry;
use super::utils::{ImageFormat, RequestLog};
use crate::conversation::message::Message;
use crate::model::ModelConfig;
use crate::providers::formats::openai::create_request;
use anyhow::Result;
use async_trait::async_trait;
use futures::future::BoxFuture;
use rmcp::model::Tool;
use serde::Deserialize;

pub const MODELS_DEV_PROVIDER_NAME: &str = "models_dev";
pub const MODELS_DEV_API_URL: &str = "https://models.dev/api.json";
pub const MODELS_DEV_API_KEY: &str = "MODELS_DEV_API_KEY";
pub const MODELS_DEV_ENDPOINT: &str = "MODELS_DEV_ENDPOINT";
pub const MODELS_DEV_DOC_URL: &str = "https://models.dev";
pub const MODELS_DEV_DEFAULT_MODEL: &str = "opencode/deepseek-v4-flash-free";

#[derive(Deserialize)]
struct ModelsDevCatalog {
    #[allow(dead_code)]
    #[serde(flatten)]
    providers: std::collections::HashMap<String, ModelsDevProviderEntry>,
}

#[derive(Deserialize)]
struct ModelsDevProviderEntry {
    #[allow(dead_code)]
    id: Option<String>,
    #[allow(dead_code)]
    name: Option<String>,
    #[allow(dead_code)]
    api: Option<String>,
    models: std::collections::HashMap<String, ModelsDevModel>,
}

#[derive(Deserialize)]
struct ModelsDevModel {
    #[allow(dead_code)]
    id: Option<String>,
    #[allow(dead_code)]
    name: Option<String>,
    cost: Option<ModelsDevCost>,
    limit: Option<ModelsDevLimit>,
    #[allow(dead_code)]
    tool_call: Option<bool>,
    #[allow(dead_code)]
    reasoning: Option<bool>,
    #[allow(dead_code)]
    attachment: Option<bool>,
}

#[derive(Deserialize)]
struct ModelsDevCost {
    #[serde(default)]
    input: f64,
    #[serde(default)]
    output: f64,
}

#[derive(Deserialize)]
struct ModelsDevLimit {
    #[serde(default)]
    context: usize,
}

#[derive(serde::Serialize)]
pub struct ModelsDevProvider {
    #[serde(skip)]
    api_client: ApiClient,
    model: ModelConfig,
    #[serde(skip)]
    name: String,
    #[serde(skip)]
    catalog_models: Vec<ModelInfo>,
}

impl ModelsDevProvider {
    async fn fetch_catalog_models() -> Result<Vec<ModelInfo>, ProviderError> {
        let response = reqwest::get(MODELS_DEV_API_URL).await.map_err(|e| {
            ProviderError::RequestFailed(format!("Failed to fetch models.dev catalog: {e}"))
        })?;

        if !response.status().is_success() {
            return Err(ProviderError::RequestFailed(format!(
                "models.dev API returned status {}",
                response.status()
            )));
        }

        let body = response.text().await.map_err(|e| {
            ProviderError::RequestFailed(format!("Failed to read models.dev response body: {e}"))
        })?;

        Self::parse_catalog_json(&body)
    }

    fn parse_catalog_json(body: &str) -> Result<Vec<ModelInfo>, ProviderError> {
        let catalog: ModelsDevCatalog = serde_json::from_str(body).map_err(|e| {
            ProviderError::RequestFailed(format!("Failed to parse models.dev catalog: {e}"))
        })?;

        let mut models: Vec<ModelInfo> = Vec::new();

        for (provider_id, provider) in &catalog.providers {
            for (model_id, model) in &provider.models {
                let context = model.limit.as_ref().map(|l| l.context).unwrap_or(4096);
                let (input_cost, output_cost) = match &model.cost {
                    Some(c) => (Some(c.input), Some(c.output)),
                    None => (None, None),
                };
                let full_id = format!("{provider_id}/{model_id}");
                models.push(ModelInfo {
                    name: full_id,
                    resolved_model: None,
                    context_limit: context,
                    input_token_cost: input_cost,
                    output_token_cost: output_cost,
                    currency: Some("$".to_string()),
                    supports_cache_control: None,
                    reasoning: model.reasoning.unwrap_or(false),
                });
            }
        }

        models.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(models)
    }

    pub async fn from_env(model: ModelConfig) -> Result<Self> {
        let config = crate::config::Config::global();
        let api_key: String = config.get_secret(MODELS_DEV_API_KEY).unwrap_or_default();

        let host: String = config
            .get_param(MODELS_DEV_ENDPOINT)
            .unwrap_or_else(|_| "https://models.dev".to_string());

        let catalog_models = Self::fetch_catalog_models().await.unwrap_or_default();

        let auth = if api_key.is_empty() {
            AuthMethod::NoAuth
        } else {
            AuthMethod::BearerToken(api_key)
        };

        let api_client = ApiClient::new(host, auth)?;

        Ok(Self {
            api_client,
            model,
            name: MODELS_DEV_PROVIDER_NAME.to_string(),
            catalog_models,
        })
    }
}

impl ProviderDef for ModelsDevProvider {
    type Provider = Self;

    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            MODELS_DEV_PROVIDER_NAME,
            "Models.dev",
            "All models from the models.dev catalog",
            MODELS_DEV_DEFAULT_MODEL,
            vec![MODELS_DEV_DEFAULT_MODEL],
            MODELS_DEV_DOC_URL,
            vec![
                ConfigKey::new(MODELS_DEV_API_KEY, false, true, None, true),
                ConfigKey::new(MODELS_DEV_ENDPOINT, false, false, None, false),
            ],
        )
    }

    fn from_env(
        model: ModelConfig,
        _extensions: Vec<crate::config::ExtensionConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>> {
        Box::pin(Self::from_env(model))
    }
}

#[async_trait]
impl Provider for ModelsDevProvider {
    fn get_name(&self) -> &str {
        &self.name
    }

    fn get_model_config(&self) -> ModelConfig {
        self.model.clone()
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(self.catalog_models.iter().map(|m| m.name.clone()).collect())
    }

    async fn fetch_supported_model_info(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        Ok(self.catalog_models.clone())
    }

    async fn fetch_model_info(&self, model_name: &str) -> Result<ModelInfo, ProviderError> {
        self.catalog_models
            .iter()
            .find(|m| m.name == model_name)
            .cloned()
            .ok_or_else(|| ProviderError::RequestFailed(format!("Unknown model: {model_name}")))
    }

    fn skip_canonical_filtering(&self) -> bool {
        true
    }

    async fn stream(
        &self,
        model_config: &ModelConfig,
        session_id: &str,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        let payload = create_request(
            model_config,
            system,
            messages,
            tools,
            &ImageFormat::OpenAi,
            true,
        )?;

        let mut log = RequestLog::start(model_config, &payload)?;

        let response = self
            .with_retry(|| async {
                let resp = self
                    .api_client
                    .response_post(Some(session_id), "chat/completions", &payload)
                    .await?;
                handle_status(resp).await
            })
            .await
            .inspect_err(|e| {
                let _ = log.error(e);
            })?;

        stream_openai_compat(response, log)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata() {
        let metadata = ModelsDevProvider::metadata();
        assert_eq!(metadata.name, MODELS_DEV_PROVIDER_NAME);
        assert_eq!(metadata.display_name, "Models.dev");
        assert!(!metadata.config_keys.is_empty());
    }

    #[test]
    fn test_parse_empty_catalog() {
        let models = ModelsDevProvider::parse_catalog_json("{}").unwrap();
        assert!(models.is_empty());
    }

    #[test]
    fn test_parse_empty_provider() {
        let json = r#"{"some_provider": {"id": "sp", "models": {}}}"#;
        let models = ModelsDevProvider::parse_catalog_json(json).unwrap();
        assert!(models.is_empty());
    }

    #[test]
    fn test_parse_single_model_no_cost_or_limit() {
        let json = r#"{
            "test-provider": {
                "id": "test-provider",
                "models": {
                    "my-model": {
                        "id": "my-model",
                        "name": "My Model"
                    }
                }
            }
        }"#;
        let models = ModelsDevProvider::parse_catalog_json(json).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].name, "test-provider/my-model");
        assert_eq!(models[0].context_limit, 4096);
        assert!(models[0].input_token_cost.is_none());
        assert!(models[0].output_token_cost.is_none());
    }

    #[test]
    fn test_parse_model_with_cost_and_limit() {
        let json = r#"{
            "p": {
                "models": {
                    "m": {
                        "cost": {"input": 0.5, "output": 1.5},
                        "limit": {"context": 128000}
                    }
                }
            }
        }"#;
        let models = ModelsDevProvider::parse_catalog_json(json).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].name, "p/m");
        assert_eq!(models[0].context_limit, 128000);
        assert_eq!(models[0].input_token_cost, Some(0.5));
        assert_eq!(models[0].output_token_cost, Some(1.5));
    }

    #[test]
    fn test_parse_multiple_providers_sorted() {
        let json = r#"{
            "z-provider": {
                "models": {
                    "z-model": {"limit": {"context": 1000}}
                }
            },
            "a-provider": {
                "models": {
                    "a-model": {"limit": {"context": 2000}}
                }
            }
        }"#;
        let models = ModelsDevProvider::parse_catalog_json(json).unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].name, "a-provider/a-model");
        assert_eq!(models[1].name, "z-provider/z-model");
    }

    #[test]
    fn test_parse_malformed_json() {
        let result = ModelsDevProvider::parse_catalog_json("not json at all");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_with_tool_call_and_reasoning() {
        let json = r#"{
            "p": {
                "models": {
                    "m": {
                        "tool_call": true,
                        "reasoning": true,
                        "limit": {"context": 32000}
                    }
                }
            }
        }"#;
        let models = ModelsDevProvider::parse_catalog_json(json).unwrap();
        assert_eq!(models.len(), 1);
        assert!(models[0].reasoning);
    }

    #[test]
    fn test_parse_zero_cost_model_still_included() {
        let json = r#"{
            "p": {
                "models": {
                    "freebie": {
                        "cost": {"input": 0, "output": 0},
                        "limit": {"context": 64000}
                    }
                }
            }
        }"#;
        let models = ModelsDevProvider::parse_catalog_json(json).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].input_token_cost, Some(0.0));
        assert_eq!(models[0].output_token_cost, Some(0.0));
    }

    #[test]
    fn test_parse_model_without_limit_defaults_to_4096() {
        let json = r#"{"p": {"models": {"m": {}}}}"#;
        let models = ModelsDevProvider::parse_catalog_json(json).unwrap();
        assert_eq!(models[0].context_limit, 4096);
    }

    #[test]
    fn test_parse_model_with_partial_cost() {
        let json = r#"{
            "p": {
                "models": {
                    "m": {
                        "cost": {"input": 0.5},
                        "limit": {"context": 4096}
                    }
                }
            }
        }"#;
        let models = ModelsDevProvider::parse_catalog_json(json).unwrap();
        assert_eq!(models[0].input_token_cost, Some(0.5));
        assert_eq!(models[0].output_token_cost, Some(0.0));
    }
}
