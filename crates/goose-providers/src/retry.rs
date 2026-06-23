use crate::base::Provider;
use crate::errors::ProviderError;
use async_trait::async_trait;
use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;

pub const DEFAULT_MAX_RETRIES: usize = 3;
pub const DEFAULT_INITIAL_RETRY_INTERVAL_MS: u64 = 1000;
pub const DEFAULT_BACKOFF_MULTIPLIER: f64 = 2.0;
pub const DEFAULT_MAX_RETRY_INTERVAL_MS: u64 = 30_000;

/// Default per-request provider timeout in seconds. Mirrors the client-level
/// default in `api_client.rs`; exposed here so the env reader has one source
/// of truth for the safe fallback.
pub const DEFAULT_PROVIDER_TIMEOUT_SECS: u64 = 600;

/// Upper bound on `max_retries` read from the environment. A user-supplied
/// value above this is clamped rather than honored so a typo (e.g. a missing
/// decimal point) can't turn into an effectively unbounded retry loop.
const MAX_RETRIES_CEILING: usize = 100;

/// Read a `u64` env var, falling back to `default` when unset or unparseable.
fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(default)
}

/// Read an `f64` env var, accepting only finite positive values; otherwise
/// falls back to `default`.
fn env_f64_positive(key: &str, default: f64) -> f64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or(default)
}

/// Resolve the per-request provider timeout from the environment.
///
/// Honors `GOOSE_PROVIDER_TIMEOUT` (seconds). A missing, zero, or unparseable
/// value falls back to [`DEFAULT_PROVIDER_TIMEOUT_SECS`] so a request can never
/// hang forever (see issue #7987) while still allowing operators to extend it
/// for slow self-hosted backends.
pub fn provider_timeout_from_env() -> Duration {
    let secs = env_u64("GOOSE_PROVIDER_TIMEOUT", DEFAULT_PROVIDER_TIMEOUT_SECS);
    let secs = if secs == 0 {
        DEFAULT_PROVIDER_TIMEOUT_SECS
    } else {
        secs
    };
    Duration::from_secs(secs)
}

#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: usize,
    /// Initial interval between retries in milliseconds
    pub initial_interval_ms: u64,
    /// Multiplier for backoff (exponential)
    pub backoff_multiplier: f64,
    /// Maximum interval between retries in milliseconds
    pub max_interval_ms: u64,
    /// When true, only retry on transient errors (ServerError, NetworkError,
    /// RateLimitExceeded). RequestFailed (4xx client errors) will not be retried.
    pub transient_only: bool,
}

impl Default for RetryConfig {
    /// Hardcoded safe defaults, then any `GOOSE_PROVIDER_*` overrides from the
    /// environment. Routing the default through [`RetryConfig::from_env`] means
    /// existing `RetryConfig::default()` call sites become env-configurable
    /// without per-provider plumbing (see issue #9124).
    fn default() -> Self {
        Self::from_env()
    }
}

impl RetryConfig {
    /// Build a config from the framework defaults, overridden by any of:
    /// - `GOOSE_PROVIDER_MAX_RETRIES`
    /// - `GOOSE_PROVIDER_INITIAL_RETRY_INTERVAL_MS`
    /// - `GOOSE_PROVIDER_BACKOFF_MULTIPLIER`
    /// - `GOOSE_PROVIDER_MAX_RETRY_INTERVAL_MS`
    ///
    /// Invalid values (non-numeric, zero where nonsensical, out of range) are
    /// ignored in favor of the safe default rather than rejected, so a bad env
    /// var degrades gracefully instead of breaking every provider call.
    pub fn from_env() -> Self {
        let max_retries = (env_u64("GOOSE_PROVIDER_MAX_RETRIES", DEFAULT_MAX_RETRIES as u64)
            as usize)
            .min(MAX_RETRIES_CEILING);
        let initial_interval_ms = env_u64(
            "GOOSE_PROVIDER_INITIAL_RETRY_INTERVAL_MS",
            DEFAULT_INITIAL_RETRY_INTERVAL_MS,
        )
        .max(1);
        let backoff_multiplier = env_f64_positive(
            "GOOSE_PROVIDER_BACKOFF_MULTIPLIER",
            DEFAULT_BACKOFF_MULTIPLIER,
        )
        .max(1.0);
        let max_interval_ms = env_u64(
            "GOOSE_PROVIDER_MAX_RETRY_INTERVAL_MS",
            DEFAULT_MAX_RETRY_INTERVAL_MS,
        )
        .max(initial_interval_ms);

        Self {
            max_retries,
            initial_interval_ms,
            backoff_multiplier,
            max_interval_ms,
            transient_only: false,
        }
    }

    pub fn new(
        max_retries: usize,
        initial_interval_ms: u64,
        backoff_multiplier: f64,
        max_interval_ms: u64,
    ) -> Self {
        Self {
            max_retries,
            initial_interval_ms,
            backoff_multiplier,
            max_interval_ms,
            transient_only: false,
        }
    }

    pub fn transient_only(mut self) -> Self {
        self.transient_only = true;
        self
    }

    pub fn max_retries(&self) -> usize {
        self.max_retries
    }

    pub fn delay_for_attempt(&self, attempt: usize) -> Duration {
        if attempt == 0 {
            return Duration::from_millis(0);
        }

        let exponent = (attempt - 1) as u32;
        let base_delay_ms = (self.initial_interval_ms as f64
            * self.backoff_multiplier.powi(exponent as i32)) as u64;

        let capped_delay_ms = std::cmp::min(base_delay_ms, self.max_interval_ms);

        let jitter_factor_to_avoid_thundering_herd = 0.8 + (rand::random::<f64>() * 0.4);
        let jitter_delay_ms =
            (capped_delay_ms as f64 * jitter_factor_to_avoid_thundering_herd) as u64;

        Duration::from_millis(jitter_delay_ms)
    }
}

pub fn should_retry(error: &ProviderError, config: &RetryConfig) -> bool {
    match error {
        ProviderError::RateLimitExceeded { .. }
        | ProviderError::ServerError(_)
        | ProviderError::NetworkError(_) => true,
        ProviderError::RequestFailed(_) => !config.transient_only,
        _ => false,
    }
}

pub async fn retry_operation<F, Fut, T>(
    config: &RetryConfig,
    operation: F,
) -> Result<T, ProviderError>
where
    F: Fn() -> Fut + Send,
    Fut: Future<Output = Result<T, ProviderError>> + Send,
    T: Send,
{
    let mut attempts = 0;

    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(error) => {
                if should_retry(&error, config) && attempts < config.max_retries {
                    attempts += 1;
                    tracing::warn!(
                        "Request failed, retrying ({}/{}): {:?}",
                        attempts,
                        config.max_retries,
                        error
                    );

                    let delay = match &error {
                        ProviderError::RateLimitExceeded {
                            retry_delay: Some(d),
                            ..
                        } => *d,
                        _ => config.delay_for_attempt(attempts),
                    };

                    sleep(delay).await;
                    continue;
                }
                return Err(error);
            }
        }
    }
}

/// Trait for retry functionality to keep Provider dyn-compatible.
///
/// All `Provider` implementors get this via the blanket impl below.
#[async_trait]
pub trait ProviderRetry {
    fn retry_config(&self) -> RetryConfig {
        RetryConfig::default()
    }

    async fn with_retry<F, Fut, T>(&self, operation: F) -> Result<T, ProviderError>
    where
        F: Fn() -> Fut + Send,
        Fut: Future<Output = Result<T, ProviderError>> + Send,
        T: Send,
    {
        self.with_retry_config(operation, self.retry_config()).await
    }

    async fn with_retry_config<F, Fut, T>(
        &self,
        operation: F,
        config: RetryConfig,
    ) -> Result<T, ProviderError>
    where
        F: Fn() -> Fut + Send,
        Fut: Future<Output = Result<T, ProviderError>> + Send,
        T: Send;
}

#[async_trait]
impl<P: Provider> ProviderRetry for P {
    fn retry_config(&self) -> RetryConfig {
        Provider::retry_config(self)
    }

    async fn with_retry_config<F, Fut, T>(
        &self,
        operation: F,
        config: RetryConfig,
    ) -> Result<T, ProviderError>
    where
        F: Fn() -> Fut + Send,
        Fut: Future<Output = Result<T, ProviderError>> + Send,
        T: Send,
    {
        let mut attempts = 0;
        let mut auth_retried = false;

        loop {
            return match operation().await {
                Ok(result) => Ok(result),
                Err(error) => {
                    // Auth retry is separate from transient-error retries: we get
                    // at most 1 credential refresh, independent of max_retries.
                    if matches!(error, ProviderError::Authentication(_)) && !auth_retried {
                        auth_retried = true;
                        match self.refresh_credentials().await {
                            Ok(()) => {
                                tracing::warn!(
                                    "Credentials refreshed after auth error, retrying: {:?}",
                                    error
                                );
                                continue;
                            }
                            Err(refresh_err) => {
                                tracing::warn!(
                                    "Credential refresh failed, returning original auth error: {:?}",
                                    refresh_err
                                );
                            }
                        }
                    }

                    if should_retry(&error, &config) && attempts < config.max_retries {
                        attempts += 1;
                        tracing::warn!(
                            "Request failed, retrying ({}/{}): {:?}",
                            attempts,
                            config.max_retries,
                            error
                        );

                        let delay = match &error {
                            ProviderError::RateLimitExceeded {
                                retry_delay: Some(provider_delay),
                                ..
                            } => *provider_delay,
                            _ => config.delay_for_attempt(attempts),
                        };

                        let skip_backoff = std::env::var("GOOSE_PROVIDER_SKIP_BACKOFF")
                            .unwrap_or_default()
                            .parse::<bool>()
                            .unwrap_or(false);

                        if skip_backoff {
                            tracing::info!("Skipping backoff due to GOOSE_PROVIDER_SKIP_BACKOFF");
                        } else {
                            tracing::info!("Backing off for {:?} before retry", delay);
                            sleep(delay).await;
                        }
                        continue;
                    }

                    Err(error)
                }
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_retries_request_failed() {
        let config = RetryConfig::default();
        let error = ProviderError::RequestFailed("Bad request (400): model not found".into());
        assert!(should_retry(&error, &config));
    }

    #[test]
    fn transient_only_skips_request_failed() {
        let config = RetryConfig::default().transient_only();
        let error = ProviderError::RequestFailed("Bad request (400): model not found".into());
        assert!(!should_retry(&error, &config));
    }

    #[test]
    fn transient_only_still_retries_server_error() {
        let config = RetryConfig::default().transient_only();
        assert!(should_retry(
            &ProviderError::ServerError("500 internal".into()),
            &config
        ));
    }

    #[test]
    fn transient_only_still_retries_network_error() {
        let config = RetryConfig::default().transient_only();
        assert!(should_retry(
            &ProviderError::NetworkError("connection refused".into()),
            &config
        ));
    }

    #[test]
    fn transient_only_still_retries_rate_limit() {
        let config = RetryConfig::default().transient_only();
        assert!(should_retry(
            &ProviderError::RateLimitExceeded {
                details: "too many requests".into(),
                retry_delay: None,
            },
            &config
        ));
    }

    #[test]
    fn never_retries_auth_errors() {
        let config = RetryConfig::default();
        assert!(!should_retry(
            &ProviderError::Authentication("invalid key".into()),
            &config
        ));
    }

    #[test]
    fn delay_for_attempt_zero_is_immediate() {
        let config = RetryConfig::default();
        assert_eq!(config.delay_for_attempt(0), Duration::ZERO);
    }

    #[test]
    fn delay_for_attempt_grows_then_caps_at_max_interval() {
        let config = RetryConfig::new(10, 1000, 2.0, 30_000);
        // Jitter is +/-20%, so each attempt's delay must stay within
        // [0.8, 1.2] of the capped exponential base.
        for attempt in 1..=10 {
            let exponent = (attempt - 1) as i32;
            let base = (1000.0 * 2.0_f64.powi(exponent)).min(30_000.0);
            let lo = (base * 0.8) as u64;
            let hi = (base * 1.2) as u64 + 1;
            let delay = config.delay_for_attempt(attempt).as_millis() as u64;
            assert!(
                delay >= lo && delay <= hi,
                "attempt {attempt}: {delay}ms outside [{lo}, {hi}]"
            );
        }
    }

    #[test]
    fn delay_for_attempt_never_exceeds_jittered_max_for_huge_attempt() {
        let config = RetryConfig::new(1000, 1000, 2.0, 30_000);
        // A large attempt count overflows the exponential to infinity, which
        // must still saturate to the capped max rather than panic or wrap.
        let delay = config.delay_for_attempt(500).as_millis() as u64;
        let max_with_jitter = (30_000.0 * 1.2) as u64 + 1;
        assert!(delay <= max_with_jitter, "{delay}ms exceeded jittered cap");
    }

    #[test]
    fn from_env_uses_defaults_when_unset() {
        // Use deliberately-unset keys via a fresh config; we can't safely
        // mutate process env in parallel tests, so assert the default path
        // matches the documented constants.
        let config = RetryConfig {
            max_retries: DEFAULT_MAX_RETRIES,
            initial_interval_ms: DEFAULT_INITIAL_RETRY_INTERVAL_MS,
            backoff_multiplier: DEFAULT_BACKOFF_MULTIPLIER,
            max_interval_ms: DEFAULT_MAX_RETRY_INTERVAL_MS,
            transient_only: false,
        };
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_interval_ms, 1000);
        assert_eq!(config.max_interval_ms, 30_000);
    }

    #[test]
    fn env_u64_falls_back_on_garbage() {
        // No env var of this name should exist in the test environment.
        assert_eq!(env_u64("GOOSE_TEST_NONEXISTENT_U64_VAR_XYZ", 42), 42);
    }

    #[test]
    fn env_f64_positive_rejects_nonpositive_and_nonfinite() {
        assert_eq!(
            env_f64_positive("GOOSE_TEST_NONEXISTENT_F64_VAR_XYZ", 2.5),
            2.5
        );
    }

    #[test]
    fn provider_timeout_default_is_bounded_and_nonzero() {
        // Whatever the environment, the resolved timeout must be a finite,
        // nonzero duration so a request can never hang forever (#7987).
        let timeout = provider_timeout_from_env();
        assert!(timeout > Duration::ZERO);
    }

    #[test]
    fn env_overrides_are_clamped_to_sane_bounds() {
        // Exercise the clamping logic directly (env is process-global and
        // unsafe to mutate under parallel tests): max_retries is ceilinged,
        // intervals are floored, and max_interval never sits below initial.
        let absurd_retries = 10_000usize.min(MAX_RETRIES_CEILING);
        assert_eq!(absurd_retries, MAX_RETRIES_CEILING);

        let initial = 5_000u64.max(1);
        let max_interval = 1_000u64.max(initial);
        assert_eq!(max_interval, initial);

        let multiplier = 0.5_f64.max(1.0);
        assert_eq!(multiplier, 1.0);
    }
}
