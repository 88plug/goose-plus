use std::path::PathBuf;
use std::sync::{Arc, RwLock};

#[cfg(feature = "aws-providers")]
use super::bedrock::BedrockProvider;
#[cfg(feature = "local-inference")]
use super::local_inference::LocalInferenceProvider;
#[cfg(feature = "aws-providers")]
use super::sagemaker_tgi::SageMakerTgiProvider;
use super::{
    amp_acp::AmpAcpProvider,
    anthropic::AnthropicProvider,
    avian::AvianProvider,
    azure::AzureProvider,
    base::{Provider, ProviderMetadata},
    chatgpt_codex::ChatGptCodexProvider,
    claude_acp::ClaudeAcpProvider,
    claude_code::ClaudeCodeProvider,
    codex::CodexProvider,
    codex_acp::CodexAcpProvider,
    copilot_acp::CopilotAcpProvider,
    cursor_agent::CursorAgentProvider,
    databricks::DatabricksProvider,
    databricks_v2::DatabricksV2Provider,
    gcpvertexai::GcpVertexAIProvider,
    gemini_cli::GeminiCliProvider,
    gemini_oauth::GeminiOAuthProvider,
    githubcopilot::GithubCopilotProvider,
    google::GoogleProvider,
    huggingface::HuggingFaceProvider,
    kimicode::KimiCodeProvider,
    litellm::LiteLLMProvider,
    models_dev::ModelsDevProvider,
    nanogpt::NanoGptProvider,
    ollama::OllamaProvider,
    openrouter::OpenRouterProvider,
    pi_acp::PiAcpProvider,
    provider_registry::ProviderRegistry,
    snowflake::SnowflakeProvider,
    tetrate::TetrateProvider,
    xai::XaiProvider,
    xai_oauth::XaiOAuthProvider,
};
use crate::config::ExtensionConfig;
use crate::providers::base::ProviderType;
use crate::providers::openai_def::OpenAiProviderDef;
use crate::{
    config::declarative_providers::register_declarative_providers,
    providers::provider_registry::ProviderEntry,
};
use anyhow::Result;
use goose_providers::model::ModelConfig;
use tokio::sync::OnceCell;

static REGISTRY: OnceCell<RwLock<ProviderRegistry>> = OnceCell::const_new();

async fn init_registry() -> RwLock<ProviderRegistry> {
    let tls_config =
        crate::config::tls::provider_tls_config_from_config(crate::config::Config::global())
            .expect("failed to load provider TLS config");
    let mut registry = ProviderRegistry::new(tls_config).with_providers(|registry| {
        use super::inventory::registrations;

        registry.register_with_inventory::<AmpAcpProvider>(
            false,
            Some(registrations::amp_acp_inventory()),
        );
        registry.register_with_inventory::<AnthropicProvider>(
            true,
            Some(registrations::anthropic_inventory()),
        );
        registry.register::<AvianProvider>(false);
        registry.register::<AzureProvider>(false);
        #[cfg(feature = "aws-providers")]
        registry.register::<BedrockProvider>(false);
        #[cfg(feature = "local-inference")]
        registry.register::<LocalInferenceProvider>(false);
        registry.register_with_inventory::<ChatGptCodexProvider>(
            true,
            Some(registrations::chatgpt_codex_inventory()),
        );
        registry.register_with_inventory::<ClaudeAcpProvider>(
            false,
            Some(registrations::claude_acp_inventory()),
        );
        registry.register::<ClaudeCodeProvider>(true);
        registry.register_with_inventory::<CodexAcpProvider>(
            false,
            Some(registrations::codex_acp_inventory()),
        );
        registry.register_with_inventory::<CopilotAcpProvider>(
            false,
            Some(registrations::copilot_acp_inventory()),
        );
        registry.register::<CodexProvider>(true);
        registry.register::<CursorAgentProvider>(false);
        registry.register_with_inventory::<DatabricksProvider>(
            true,
            Some(registrations::refresh_only()),
        );
        registry.register_with_inventory::<DatabricksV2Provider>(
            false,
            Some(registrations::refresh_only()),
        );
        registry.register::<GcpVertexAIProvider>(false);
        registry.register::<GeminiCliProvider>(false);
        registry.register::<GeminiOAuthProvider>(true);
        registry.register::<GithubCopilotProvider>(false);
        registry.register_with_inventory::<GoogleProvider>(
            true,
            Some(registrations::google_inventory()),
        );
        registry.register_with_inventory::<HuggingFaceProvider>(
            true,
            Some(registrations::huggingface_inventory()),
        );
        registry.register::<KimiCodeProvider>(true);
        registry.register::<LiteLLMProvider>(false);
        registry.register::<ModelsDevProvider>(true);
        registry.register::<NanoGptProvider>(true);
        registry.register_with_inventory::<OllamaProvider>(
            true,
            Some(registrations::ollama_inventory()),
        );
        registry.register_with_inventory::<OpenAiProviderDef>(
            true,
            Some(registrations::openai_inventory()),
        );
        registry.register::<OpenRouterProvider>(true);
        registry.register_with_inventory::<PiAcpProvider>(
            false,
            Some(registrations::pi_acp_inventory()),
        );
        #[cfg(feature = "aws-providers")]
        registry.register::<SageMakerTgiProvider>(false);
        registry.register::<SnowflakeProvider>(false);
        registry.register::<TetrateProvider>(true);
        registry.register::<XaiProvider>(false);
        registry.register_with_inventory::<XaiOAuthProvider>(
            true,
            Some(registrations::xai_oauth_inventory()),
        );
    });
    // Register cleanup functions for providers with cached state
    registry.set_cleanup(
        "github_copilot",
        Arc::new(|| Box::pin(GithubCopilotProvider::cleanup())),
    );
    registry.set_cleanup(
        "databricks",
        Arc::new(|| Box::pin(DatabricksProvider::cleanup())),
    );
    registry.set_cleanup(
        "databricks_v2",
        Arc::new(|| Box::pin(DatabricksV2Provider::cleanup())),
    );
    registry.set_cleanup(
        "kimi_code",
        Arc::new(|| Box::pin(KimiCodeProvider::cleanup())),
    );
    registry.set_cleanup(
        "chatgpt_codex",
        Arc::new(|| Box::pin(ChatGptCodexProvider::cleanup())),
    );
    registry.set_cleanup(
        "gemini_oauth",
        Arc::new(|| Box::pin(GeminiOAuthProvider::cleanup())),
    );
    registry.set_cleanup(
        "xai_oauth",
        Arc::new(|| Box::pin(XaiOAuthProvider::cleanup())),
    );
    registry.set_cleanup(
        "huggingface",
        Arc::new(|| Box::pin(HuggingFaceProvider::cleanup())),
    );

    if let Err(e) = load_custom_providers_into_registry(&mut registry) {
        tracing::warn!("Failed to load custom providers: {}", e);
    }
    RwLock::new(registry)
}

fn load_custom_providers_into_registry(registry: &mut ProviderRegistry) -> Result<()> {
    register_declarative_providers(registry)
}

async fn get_registry() -> &'static RwLock<ProviderRegistry> {
    REGISTRY.get_or_init(init_registry).await
}

pub async fn providers() -> Vec<(ProviderMetadata, ProviderType)> {
    get_registry()
        .await
        .read()
        .unwrap()
        .all_metadata_with_types()
}

pub async fn refresh_custom_providers() -> Result<()> {
    let registry = get_registry().await;
    registry.write().unwrap().remove_custom_providers();

    if let Err(e) = load_custom_providers_into_registry(&mut registry.write().unwrap()) {
        tracing::warn!("Failed to refresh custom providers: {}", e);
        return Err(e);
    }

    tracing::info!("Custom providers refreshed");
    Ok(())
}

pub async fn get_from_registry(name: &str) -> Result<ProviderEntry> {
    let guard = get_registry().await.read().unwrap();
    guard
        .entries
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("Unknown provider: {}", name))
        .cloned()
}

pub async fn inventory_identity(name: &str) -> Result<super::inventory::InventoryIdentityInput> {
    get_from_registry(name).await?.inventory_identity()
}

pub async fn create(
    name: &str,
    model: ModelConfig,
    extensions: Vec<ExtensionConfig>,
) -> Result<Arc<dyn Provider>> {
    let entry = get_from_registry(name).await?;
    entry.create(model, extensions).await
}

pub async fn create_with_working_dir(
    name: &str,
    model: ModelConfig,
    extensions: Vec<ExtensionConfig>,
    working_dir: PathBuf,
) -> Result<Arc<dyn Provider>> {
    let entry = get_from_registry(name).await?;
    entry
        .create_with_working_dir(model, extensions, working_dir)
        .await
}

pub async fn create_with_default_model(
    name: impl AsRef<str>,
    extensions: Vec<ExtensionConfig>,
) -> Result<Arc<dyn Provider>> {
    get_from_registry(name.as_ref())
        .await?
        .create_with_default_model(extensions)
        .await
}

pub async fn cleanup_provider(name: &str) -> Result<()> {
    let cleanup_fn = {
        let registry = get_registry().await.read().unwrap();
        registry
            .entries
            .get(name)
            .and_then(|entry| entry.cleanup.clone())
    };
    if let Some(cleanup) = cleanup_fn {
        return cleanup().await;
    }
    Ok(())
}

pub async fn create_with_named_model(
    provider_name: &str,
    model_name: &str,
    extensions: Vec<ExtensionConfig>,
) -> Result<Arc<dyn Provider>> {
    let config = crate::model_config::model_config_from_user_config(provider_name, model_name)?;
    create(provider_name, config, extensions).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::paths::Paths;
    use std::fs;

    #[tokio::test]
    async fn test_tanzu_declarative_provider_registry_wiring() {
        let providers_list = providers().await;
        let tanzu = providers_list
            .iter()
            .find(|(m, _)| m.name == "tanzu_ai")
            .expect("tanzu_ai provider should be registered");
        let (meta, provider_type) = tanzu;

        // Should be a Declarative (fixed) provider
        assert_eq!(*provider_type, ProviderType::Declarative);

        assert_eq!(meta.display_name, "VMware Tanzu Platform");
        assert_eq!(meta.default_model, "openai/gpt-oss-120b");

        // First config key should be TANZU_AI_API_KEY (secret, required)
        let api_key = meta
            .config_keys
            .iter()
            .find(|k| k.name == "TANZU_AI_API_KEY")
            .expect("TANZU_AI_API_KEY config key should exist");
        assert!(
            api_key.required,
            "API key should be required for fixed declarative provider"
        );
        assert!(api_key.secret, "API key should be secret");

        // Should have TANZU_AI_ENDPOINT config key (not secret, required)
        let endpoint = meta
            .config_keys
            .iter()
            .find(|k| k.name == "TANZU_AI_ENDPOINT")
            .expect("TANZU_AI_ENDPOINT config key should exist");
        assert!(endpoint.required, "Endpoint should be required");
        assert!(!endpoint.secret, "Endpoint should not be secret");
    }

    #[tokio::test]
    async fn test_huggingface_provider_registry_wiring() {
        let huggingface = get_from_registry("huggingface")
            .await
            .expect("huggingface provider should be registered");
        let meta = huggingface.metadata();

        assert_eq!(huggingface.provider_type(), ProviderType::Preferred);
        assert_eq!(meta.display_name, "Hugging Face");
        assert_eq!(meta.default_model, "Qwen/Qwen3-Coder-480B-A35B-Instruct");
        assert!(meta
            .config_keys
            .iter()
            .any(|key| key.name == "HF_TOKEN" && key.secret));
    }

    #[tokio::test]
    async fn test_nvidia_declarative_provider_registry_wiring() {
        let nvidia = get_from_registry("nvidia")
            .await
            .expect("nvidia provider should be registered");
        let meta = nvidia.metadata();

        assert_eq!(nvidia.provider_type(), ProviderType::Declarative);
        assert!(nvidia.supports_inventory_refresh());
        assert_eq!(meta.display_name, "NVIDIA");
        assert_eq!(meta.default_model, "z-ai/glm-4.7");
        assert_eq!(meta.model_doc_link, "https://build.nvidia.com/models");
        assert!(!meta.setup_steps.is_empty());

        let api_key = meta
            .config_keys
            .iter()
            .find(|k| k.name == "NVIDIA_API_KEY")
            .expect("NVIDIA_API_KEY config key should exist");
        assert!(api_key.required, "NVIDIA_API_KEY should be required");
        assert!(api_key.secret, "NVIDIA_API_KEY should be secret");
        assert!(api_key.primary, "NVIDIA_API_KEY should be primary");
        assert!(
            !meta.config_keys.iter().any(|k| k.name == "OPENAI_HOST"),
            "NVIDIA should not expose OpenAI host configuration"
        );
        assert!(
            !meta
                .config_keys
                .iter()
                .any(|k| k.name == "OPENAI_BASE_PATH"),
            "NVIDIA should not expose OpenAI base path configuration"
        );
    }

    #[tokio::test]
    async fn test_nearai_declarative_provider_registry_wiring() {
        let nearai = get_from_registry("nearai")
            .await
            .expect("nearai provider should be registered");
        let meta = nearai.metadata();

        assert_eq!(nearai.provider_type(), ProviderType::Declarative);
        assert!(nearai.supports_inventory_refresh());
        assert_eq!(meta.display_name, "NEAR AI Cloud");
        assert_eq!(meta.default_model, "zai-org/GLM-5.1-FP8");
        assert_eq!(meta.model_doc_link, "https://docs.near.ai/");
        assert!(!meta.setup_steps.is_empty());

        let api_key = meta
            .config_keys
            .iter()
            .find(|k| k.name == "NEARAI_API_KEY")
            .expect("NEARAI_API_KEY config key should exist");
        assert!(api_key.required, "NEARAI_API_KEY should be required");
        assert!(api_key.secret, "NEARAI_API_KEY should be secret");
        assert!(api_key.primary, "NEARAI_API_KEY should be primary");
    }

    #[tokio::test]
    async fn test_alibaba_declarative_provider_registry_wiring() {
        let alibaba = get_from_registry("alibaba")
            .await
            .expect("alibaba provider should be registered");
        let meta = alibaba.metadata();

        assert_eq!(alibaba.provider_type(), ProviderType::Declarative);
        assert!(alibaba.supports_inventory_refresh());
        assert_eq!(meta.display_name, "Alibaba (Qwen)");
        assert_eq!(meta.default_model, "qwen3.7-max");
        assert_eq!(
            meta.model_doc_link,
            "https://www.alibabacloud.com/help/en/model-studio/models"
        );
        assert!(!meta.setup_steps.is_empty());

        let api_key = meta
            .config_keys
            .iter()
            .find(|k| k.name == "DASHSCOPE_API_KEY")
            .expect("DASHSCOPE_API_KEY config key should exist");
        assert!(api_key.required, "DASHSCOPE_API_KEY should be required");
        assert!(api_key.secret, "DASHSCOPE_API_KEY should be secret");
        assert!(api_key.primary, "DASHSCOPE_API_KEY should be primary");
    }

    #[tokio::test]
    async fn test_openai_compatible_providers_config_keys() {
        let providers_list = providers().await;
        let required_api_key_cases = vec![
            ("groq", "GROQ_API_KEY"),
            ("mistral", "MISTRAL_API_KEY"),
            ("custom_deepseek", "DEEPSEEK_API_KEY"),
        ];
        for (name, expected_key) in required_api_key_cases {
            if let Some((meta, _)) = providers_list.iter().find(|(m, _)| m.name == name) {
                assert!(
                    !meta.config_keys.is_empty(),
                    "{name} provider should have config keys"
                );
                assert_eq!(
                    meta.config_keys[0].name, expected_key,
                    "First config key for {name} should be {expected_key}, got {}",
                    meta.config_keys[0].name
                );
                assert!(
                    meta.config_keys[0].required,
                    "{expected_key} should be required"
                );
                assert!(
                    meta.config_keys[0].secret,
                    "{expected_key} should be secret"
                );
            } else {
                // Provider not registered; skip test for this provider
                continue;
            }
        }

        if let Some((meta, _)) = providers_list.iter().find(|(m, _)| m.name == "openai") {
            assert!(
                !meta.config_keys.is_empty(),
                "openai provider should have config keys"
            );
            assert_eq!(
                meta.config_keys[0].name, "OPENAI_API_KEY",
                "First config key for openai should be OPENAI_API_KEY"
            );
            assert!(
                !meta.config_keys[0].required,
                "OPENAI_API_KEY should be optional for local server support"
            );
            assert!(
                meta.config_keys[0].secret,
                "OPENAI_API_KEY should be secret"
            );
        }
    }

    #[tokio::test]
    async fn test_custom_provider_context_limit_is_applied_from_file() {
        let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", None::<&str>)]);
        let temp_dir = tempfile::tempdir().expect("tempdir should be created");
        std::env::set_var("GOOSE_PATH_ROOT", temp_dir.path());

        let custom_dir = Paths::config_dir().join("custom_providers");
        fs::create_dir_all(&custom_dir).expect("custom providers dir should be created");

        let custom_inf = r#"{
  "name": "custom_inf",
  "engine": "openai",
  "display_name": "Custom Inf",
  "description": "test provider",
  "api_key_env": "",
  "base_url": "https://example.invalid/v1/chat/completions",
  "models": [
    {"name": "kimi-k2.5", "context_limit": 256000}
  ],
  "requires_auth": false
}"#;
        fs::write(custom_dir.join("custom_inf.json"), custom_inf)
            .expect("custom_inf.json should be written");

        let custom_zero = r#"{
  "name": "custom_zero",
  "engine": "openai",
  "display_name": "Custom Zero",
  "description": "test provider",
  "api_key_env": "",
  "base_url": "https://example.invalid/v1/chat/completions",
  "models": [
    {"name": "zero-model", "context_limit": 0}
  ],
  "requires_auth": false
}"#;
        fs::write(custom_dir.join("custom_zero.json"), custom_zero)
            .expect("custom_zero.json should be written");

        refresh_custom_providers()
            .await
            .expect("custom providers should refresh");

        let provider = create_with_named_model("custom_inf", "kimi-k2.5", Vec::new())
            .await
            .expect("custom_inf provider should be creatable");
        assert_eq!(provider.get_model_config().context_limit, Some(256_000));

        let zero_provider = create_with_named_model("custom_zero", "zero-model", Vec::new())
            .await
            .expect("custom_zero provider should be creatable");
        assert_eq!(zero_provider.get_model_config().context_limit, None);

        std::env::remove_var("GOOSE_PATH_ROOT");
    }

    #[tokio::test]
    async fn test_opencode_declarative_provider_registry_wiring() {
        let providers_list = providers().await;
        let opencode = providers_list
            .iter()
            .find(|(m, _)| m.name == "opencode")
            .expect("opencode provider should be registered");
        let (meta, provider_type) = opencode;

        assert_eq!(*provider_type, ProviderType::Declarative);
        assert_eq!(meta.display_name, "OpenCode");
        assert_eq!(
            meta.default_model,
            "deepseek-v4-flash-free",
            "default model should be the first free model"
        );
        assert_eq!(
            meta.model_doc_link,
            "https://opencode.ai/docs/zen"
        );

        // API key is optional (requires_auth: false)
        let api_key = meta
            .config_keys
            .iter()
            .find(|k| k.name == "OPENCODE_API_KEY")
            .expect("OPENCODE_API_KEY config key should exist");
        assert!(
            !api_key.required,
            "OPENCODE_API_KEY should NOT be required"
        );
        assert!(api_key.secret, "OPENCODE_API_KEY should be secret");
        assert!(api_key.primary, "OPENCODE_API_KEY should be primary");

        // Verify free models are included in known_models
        let free = meta
            .known_models
            .iter()
            .find(|m| m.name == "deepseek-v4-flash-free")
            .expect("deepseek-v4-flash-free should be in known_models");
        assert_eq!(free.context_limit, 200000);
        assert_eq!(free.input_token_cost, Some(0.0));
        assert_eq!(free.output_token_cost, Some(0.0));
    }

    #[test]
    fn test_opencode_json_deserializes() {
        let json = include_str!("declarative/opencode.json");
        let config: crate::config::declarative_providers::DeclarativeProviderConfig =
            serde_json::from_str(json).expect("opencode.json should parse");
        assert_eq!(config.name, "opencode");
        assert!(matches!(config.engine, crate::config::declarative_providers::ProviderEngine::OpenAI));
        assert_eq!(config.base_url, "https://opencode.ai/zen/v1");
        assert!(!config.requires_auth, "free tier should not require auth");
        assert!(config.dynamic_models.unwrap_or(false));
        assert_eq!(config.fast_model.as_deref(), Some("deepseek-v4-flash-free"));

        // Verify all 42 models are present (4 free + 38 paid, matching /zen/v1/models)
        assert_eq!(config.models.len(), 42);
        assert!(config.models.iter().any(|m| m.name == "deepseek-v4-flash-free"));
        assert!(config.models.iter().any(|m| m.name == "gpt-5-nano"));
        // Free models have cost 0; paid models have non-zero costs
        let free: Vec<_> = config.models.iter().filter(|m| m.input_token_cost == Some(0.0)).collect();
        assert_eq!(free.len(), 4, "expected 4 free models");
    }

    #[tokio::test]
    async fn test_models_dev_provider_registry_wiring() {
        let models_dev = get_from_registry("models_dev")
            .await
            .expect("models_dev provider should be registered");
        let meta = models_dev.metadata();

        assert_eq!(models_dev.provider_type(), ProviderType::Preferred);
        assert_eq!(meta.display_name, "Models.dev");
        assert!(meta.config_keys.iter().any(|k| k.name == "MODELS_DEV_API_KEY"));
        assert!(meta.config_keys.iter().any(|k| k.name == "MODELS_DEV_ENDPOINT"));
    }
}
