//! Shared utilities for xAI provider routing and model handling.
//! Core logic ported from packages/ai/src/providers/xai-shared.ts in pi.

use std::collections::HashSet;
use std::sync::LazyLock;

/// xAI API base URLs
pub const XAI_API_BASE_URL: &str = "https://api.x.ai/v1";
pub const XAI_CLI_BASE_URL: &str = "https://cli-chat-proxy.grok.com/v1";

/// Grok CLI client version header
pub const XAI_GROK_CLIENT_VERSION: &str = "0.2.16";

/// Models that must route through xAI's Grok CLI proxy instead of the public Responses API.
/// Note: grok-build-0.1 uses the public api.x.ai Responses API, not the CLI proxy.
pub static XAI_GROK_CLI_PROXY_MODEL_IDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    ["grok-build", "grok-composer-2.5-fast"]
        .into_iter()
        .collect()
});

/// Authoritative context windows for native xAI Grok models.
/// Sourced from xAI `GET /v1/models` `context_length` (OAuth/API key).
/// These are used by the xAI providers when canonical data is missing or stale.
pub static XAI_CONTEXT_WINDOWS: LazyLock<std::collections::HashMap<&'static str, usize>> =
    LazyLock::new(|| {
        let mut m = std::collections::HashMap::new();
        // Standard models
        m.insert("grok-3", 131072);
        m.insert("grok-3-fast", 131072);
        // Grok 4 Fast lineage – 2M tier (published, matches pi spec)
        m.insert("grok-4.20-0309-non-reasoning", 2_000_000);
        m.insert("grok-4.20-0309-reasoning", 2_000_000);
        m.insert("grok-4.20-multi-agent-0309", 2_000_000);
        m.insert("grok-4.3", 2_000_000);
        // Grok Build / Composer (CLI proxy or public)
        m.insert("grok-build", 512_000);
        m.insert("grok-build-0.1", 256_000);
        m.insert("grok-composer-2.5-fast", 200_000);
        // Code fast
        m.insert("grok-code-fast-1", 256_000);
        m
    });

/// Return the published context window for a Grok model id, if known.
pub fn xai_context_window(model_id: &str) -> Option<usize> {
    let norm = normalized_xai_model_id(model_id);
    XAI_CONTEXT_WINDOWS.get(norm.as_str()).copied()
}

/// Normalize provider/model-prefixed xAI model ids for routing comparisons.
pub fn normalized_xai_model_id(model_id: &str) -> String {
    model_id
        .to_lowercase()
        .split('/')
        .next_back()
        .unwrap_or("")
        .to_string()
}

/// Return true for models that must route through xAI's Grok CLI proxy.
pub fn is_grok_cli_proxy_model(model_id: &str) -> bool {
    XAI_GROK_CLI_PROXY_MODEL_IDS.contains(normalized_xai_model_id(model_id).as_str())
}

/// Return true for xAI "multi-agent" fan-out models.
/// These run their own internal tools and reject client-side tool definitions unless
/// the account has beta access.
pub fn is_xai_multi_agent_model(model_id: &str) -> bool {
    let normalized = normalized_xai_model_id(model_id);
    normalized.contains("multi-agent") || normalized.contains("multi_agent")
}

/// Return true when xAI accepts client-side tool definitions for a model.
pub fn grok_supports_client_side_tools(model_id: &str) -> bool {
    !is_xai_multi_agent_model(model_id)
}

/// Resolve the base URL used by a model.
pub fn xai_base_url_for_model(model_id: &str) -> &'static str {
    if is_grok_cli_proxy_model(model_id) {
        XAI_CLI_BASE_URL
    } else {
        XAI_API_BASE_URL
    }
}

/// Build Grok CLI proxy headers for Composer/Grok Build requests.
pub fn grok_cli_proxy_headers(
    model_id: &str,
    session_id: Option<&str>,
) -> std::collections::HashMap<String, String> {
    let mut headers = std::collections::HashMap::new();
    headers.insert("x-grok-client-identifier".to_string(), "goose".to_string());
    headers.insert(
        "x-grok-client-version".to_string(),
        XAI_GROK_CLIENT_VERSION.to_string(),
    );
    headers.insert("x-xai-token-auth".to_string(), "xai-grok-cli".to_string());
    headers.insert(
        "x-grok-model-override".to_string(),
        normalized_xai_model_id(model_id),
    );
    if let Some(sid) = session_id {
        headers.insert("x-grok-conv-id".to_string(), sid.to_string());
    }
    headers
}

/// Build extra request headers needed for a given xAI model.
pub fn xai_model_request_headers(
    model_id: &str,
    session_id: Option<&str>,
) -> std::collections::HashMap<String, String> {
    if is_grok_cli_proxy_model(model_id) {
        grok_cli_proxy_headers(model_id, session_id)
    } else {
        std::collections::HashMap::new()
    }
}

/// Return true when xAI accepts an explicit Responses `reasoning.effort`.
///
/// Only grok-4.3 and the multi-agent fan-out models take an explicit effort.
/// Other reasoning-capable models (grok-4.20-0309-reasoning, grok-build, grok-build-0.1)
/// reason automatically and reject an explicit effort.
pub fn grok_supports_reasoning_effort(model_id: &str) -> bool {
    let normalized = normalized_xai_model_id(model_id);
    normalized.starts_with("grok-4.3") || is_xai_multi_agent_model(model_id)
}

/// Mapping from generic thinking levels to xAI Responses effort values.
/// Only models that support explicit effort use this.
pub fn xai_reasoning_effort_for_thinking(thinking_level: &str) -> Option<&'static str> {
    match thinking_level {
        "minimal" => Some("low"),
        "low" => Some("low"),
        "medium" => Some("medium"),
        "high" => Some("high"),
        "xhigh" => Some("high"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalized_model_id() {
        assert_eq!(normalized_xai_model_id("xai/grok-build"), "grok-build");
        assert_eq!(
            normalized_xai_model_id("Grok-Composer-2.5-Fast"),
            "grok-composer-2.5-fast"
        );
    }

    #[test]
    fn test_cli_proxy_routing() {
        assert!(is_grok_cli_proxy_model("grok-build"));
        assert!(is_grok_cli_proxy_model("grok-composer-2.5-fast"));
        assert!(!is_grok_cli_proxy_model("grok-build-0.1"));
        assert!(!is_grok_cli_proxy_model("grok-4.3"));
    }

    #[test]
    fn test_multi_agent_detection() {
        assert!(is_xai_multi_agent_model("grok-4.20-multi-agent-0309"));
        assert!(!is_xai_multi_agent_model("grok-4.3"));
    }

    #[test]
    fn test_reasoning_effort_gating() {
        assert!(grok_supports_reasoning_effort("grok-4.3"));
        assert!(grok_supports_reasoning_effort("grok-4.20-multi-agent-0309"));
        assert!(!grok_supports_reasoning_effort("grok-4.20-0309-reasoning"));
        assert!(!grok_supports_reasoning_effort("grok-build"));
    }

    #[test]
    fn test_base_url_resolution() {
        assert_eq!(xai_base_url_for_model("grok-build"), XAI_CLI_BASE_URL);
        assert_eq!(xai_base_url_for_model("grok-4.3"), XAI_API_BASE_URL);
    }

    #[test]
    fn test_context_windows_match_live_api() {
        // Grok 4 Fast lineage = 2M (published tier, no 1M guesses)
        assert_eq!(xai_context_window("grok-4.3"), Some(2_000_000));
        assert_eq!(
            xai_context_window("grok-4.20-0309-reasoning"),
            Some(2_000_000)
        );
        assert_eq!(xai_context_window("grok-build-0.1"), Some(256_000));
        assert_eq!(xai_context_window("grok-build"), Some(512_000));
        assert_eq!(xai_context_window("grok-composer-2.5-fast"), Some(200_000));
    }
}
