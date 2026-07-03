use super::api_client::TlsConfig;
use anyhow::Result;
use futures::future::BoxFuture;
pub use goose_providers::conversation::token_usage::{
    DraftStats, ProviderStats, ProviderUsage, Usage,
};
use serde::{Deserialize, Serialize};

pub const DEFAULT_PROVIDER_TIMEOUT_SECS: u64 = 600;

use crate::config::ExtensionConfig;
use goose_providers::model::ModelConfig;
use utoipa::ToSchema;

use std::path::PathBuf;

pub use goose_providers::base::*;

/// Resolve a provider's request timeout, in priority order: a provider-specific
/// config/env key (e.g. `OPENAI_TIMEOUT`), the global `GOOSE_PROVIDER_TIMEOUT`
/// override, then [`DEFAULT_PROVIDER_TIMEOUT_SECS`]. A zero value at any tier is
/// treated as unset so a stray `0` can't turn into a request that never times out.
pub fn resolve_provider_timeout(provider_timeout_key: Option<&str>) -> std::time::Duration {
    let config = crate::config::Config::global();
    let timeout_secs = provider_timeout_key
        .and_then(|key| config.get_param::<u64>(key).ok())
        .filter(|seconds| *seconds > 0)
        .or_else(|| {
            config
                .get_param::<u64>("GOOSE_PROVIDER_TIMEOUT")
                .ok()
                .filter(|seconds| *seconds > 0)
        })
        .unwrap_or(DEFAULT_PROVIDER_TIMEOUT_SECS);

    std::time::Duration::from_secs(timeout_secs)
}

#[cfg(test)]
mod resolve_provider_timeout_tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn defaults_when_nothing_set() {
        std::env::remove_var("GOOSE_PROVIDER_TIMEOUT");
        std::env::remove_var("OPENAI_TIMEOUT");
        assert_eq!(
            resolve_provider_timeout(Some("OPENAI_TIMEOUT")),
            std::time::Duration::from_secs(DEFAULT_PROVIDER_TIMEOUT_SECS)
        );
    }

    #[test]
    #[serial]
    fn honors_global_override_when_no_provider_specific_key() {
        std::env::remove_var("OPENAI_TIMEOUT");
        std::env::set_var("GOOSE_PROVIDER_TIMEOUT", "123");
        assert_eq!(
            resolve_provider_timeout(None),
            std::time::Duration::from_secs(123)
        );
        std::env::remove_var("GOOSE_PROVIDER_TIMEOUT");
    }

    #[test]
    #[serial]
    fn provider_specific_key_takes_priority_over_global() {
        std::env::set_var("OPENAI_TIMEOUT", "321");
        std::env::set_var("GOOSE_PROVIDER_TIMEOUT", "123");
        assert_eq!(
            resolve_provider_timeout(Some("OPENAI_TIMEOUT")),
            std::time::Duration::from_secs(321)
        );
        std::env::remove_var("OPENAI_TIMEOUT");
        std::env::remove_var("GOOSE_PROVIDER_TIMEOUT");
    }

    #[test]
    #[serial]
    fn falls_back_to_global_when_provider_specific_is_zero() {
        std::env::set_var("OPENAI_TIMEOUT", "0");
        std::env::set_var("GOOSE_PROVIDER_TIMEOUT", "123");
        assert_eq!(
            resolve_provider_timeout(Some("OPENAI_TIMEOUT")),
            std::time::Duration::from_secs(123)
        );
        std::env::remove_var("OPENAI_TIMEOUT");
        std::env::remove_var("GOOSE_PROVIDER_TIMEOUT");
    }

    #[test]
    #[serial]
    fn falls_back_to_default_when_global_is_also_zero() {
        std::env::set_var("OPENAI_TIMEOUT", "0");
        std::env::set_var("GOOSE_PROVIDER_TIMEOUT", "0");
        assert_eq!(
            resolve_provider_timeout(Some("OPENAI_TIMEOUT")),
            std::time::Duration::from_secs(DEFAULT_PROVIDER_TIMEOUT_SECS)
        );
        std::env::remove_var("OPENAI_TIMEOUT");
        std::env::remove_var("GOOSE_PROVIDER_TIMEOUT");
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum ProviderType {
    Preferred,
    Builtin,
    Declarative,
    Custom,
}

pub(crate) fn current_working_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub trait ProviderDef: ProviderDescriptor + Send + Sync {
    type Provider: Provider + 'static;

    fn from_env(
        model: ModelConfig,
        extensions: Vec<ExtensionConfig>,
        tls_config: Option<TlsConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>>
    where
        Self: Sized;

    fn from_env_with_working_dir(
        model: ModelConfig,
        extensions: Vec<ExtensionConfig>,
        _working_dir: PathBuf,
        tls_config: Option<TlsConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>>
    where
        Self: Sized,
    {
        Self::from_env(model, extensions, tls_config)
    }
}
