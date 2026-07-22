pub mod adversary_inspector;
pub mod classification_client;
pub mod egress_inspector;
pub mod patterns;
pub mod scanner;
pub mod security_inspector;
pub mod text_normalizer;

use crate::config::Config;
use crate::conversation::message::{Message, ToolRequest};
use crate::permission::permission_judge::PermissionCheckResult;
use crate::session::diagnostics::redact_secrets;
use anyhow::Result;
use scanner::PromptInjectionScanner;
use std::env;
use std::sync::OnceLock;
use uuid::Uuid;

fn redacted_tool_call_json(tool_call: &rmcp::model::CallToolRequestParams) -> String {
    // Pretty-print before redacting: redact_secrets is line-oriented and isolates
    // one `"key": "value"` per line. On compact single-line JSON a stray `": "`
    // inside one argument hijacks the scan (early-returning past an earlier
    // secret), and space-bearing values like `"Bearer <token>"` never match.
    // One field per line closes both gaps.
    redact_secrets(&serde_json::to_string_pretty(tool_call).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) fn get_override(env_key: &str) -> Option<bool> {
    env::var(env_key).ok().and_then(|v| match v.as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    })
}

pub struct SecurityManager {
    scanner: OnceLock<PromptInjectionScanner>,
}

#[derive(Debug, Clone)]
pub struct SecurityResult {
    pub is_malicious: bool,
    pub confidence: f32,
    pub explanation: String,
    pub should_ask_user: bool,
    pub finding_id: String,
    pub tool_request_id: String,
}

impl SecurityManager {
    pub fn new() -> Self {
        Self {
            scanner: OnceLock::new(),
        }
    }

    pub fn is_prompt_injection_detection_enabled(&self) -> bool {
        if let Some(overridden) = get_override("SECURITY_PROMPT_ENABLED_OVERRIDE") {
            return overridden;
        }

        let config = Config::global();
        config
            .get_param::<bool>("SECURITY_PROMPT_ENABLED")
            .unwrap_or(false)
    }

    fn is_ml_scanning_enabled(&self) -> bool {
        let config = Config::global();

        let prompt_enabled = config
            .get_param::<bool>("SECURITY_PROMPT_CLASSIFIER_ENABLED")
            .unwrap_or(false);

        let command_enabled = if let Some(overridden) =
            get_override("SECURITY_COMMAND_CLASSIFIER_ENABLED_OVERRIDE")
        {
            overridden
        } else {
            config
                .get_param::<bool>("SECURITY_COMMAND_CLASSIFIER_ENABLED")
                .unwrap_or(false)
        };

        prompt_enabled || command_enabled
    }

    pub async fn analyze_tool_requests(
        &self,
        tool_requests: &[ToolRequest],
        messages: &[Message],
    ) -> Result<Vec<SecurityResult>> {
        if !self.is_prompt_injection_detection_enabled() {
            tracing::debug!(
                monotonic_counter.goose.prompt_injection_scanner_disabled = 1,
                "Security scanning disabled"
            );
            return Ok(vec![]);
        }

        let scanner = self.scanner.get_or_init(|| {
            let config = Config::global();
            let command_classifier_enabled =
                if let Some(overridden) = get_override("SECURITY_COMMAND_CLASSIFIER_ENABLED_OVERRIDE") {
                    overridden
                } else {
                    config
                        .get_param::<bool>("SECURITY_COMMAND_CLASSIFIER_ENABLED")
                        .unwrap_or(false)
                };
            let prompt_classifier_enabled = config
                .get_param::<bool>("SECURITY_PROMPT_CLASSIFIER_ENABLED")
                .unwrap_or(false);

            tracing::info!(
                monotonic_counter.goose.security_command_classifier_enabled = if command_classifier_enabled { 1 } else { 0 },
                monotonic_counter.goose.security_prompt_classifier_enabled = if prompt_classifier_enabled { 1 } else { 0 },
                "Security classifier configuration"
            );

            let ml_enabled = self.is_ml_scanning_enabled();

            let scanner = if ml_enabled {
                match PromptInjectionScanner::with_ml_detection() {
                    Ok(s) => {
                        tracing::info!(
                            monotonic_counter.goose.prompt_injection_scanner_enabled = 1,
                            "Security scanner initialized with ML-based detection"
                        );
                        s
                    }
                    Err(e) => {
                        let error_chain = format!("{:#}", e);
                        tracing::warn!(
                            "ML scanning requested but failed to initialize. Falling back to pattern-only scanning.\n\nError details:\n{}",
                            error_chain
                        );
                        PromptInjectionScanner::new()
                    }
                }
            } else {
                tracing::info!(
                    monotonic_counter.goose.prompt_injection_scanner_enabled = 1,
                    "Security scanner initialized with pattern-based detection only"
                );
                PromptInjectionScanner::new()
            };

            scanner
        });

        let mut results = Vec::new();

        tracing::debug!(
            "Starting security analysis - {} tool requests, {} messages",
            tool_requests.len(),
            messages.len()
        );

        for tool_request in tool_requests.iter() {
            if let Ok(tool_call) = &tool_request.tool_call {
                let analysis_result = scanner
                    .analyze_tool_call_with_context(tool_call, messages)
                    .await?;

                let config_threshold = scanner.get_threshold_from_config();
                let sanitized_explanation = analysis_result.explanation.replace('\n', " | ");

                if analysis_result.is_malicious {
                    let above_threshold = analysis_result.confidence > config_threshold;
                    let finding_id = format!("SEC-{}", Uuid::new_v4().simple());

                    let tool_call_json = redacted_tool_call_json(tool_call);

                    let action = if above_threshold { "BLOCK" } else { "LOG" };

                    tracing::warn!(
                        monotonic_counter.goose.prompt_injection_finding = 1,
                        security.event_type = "prompt_injection_scan",
                        security.action = action,
                        security.confidence = analysis_result.confidence,
                        security.threshold = config_threshold,
                        security.above_threshold = above_threshold,
                        security.threat_type = "command_injection",
                        security.finding_id = %finding_id,
                        security.explanation = %sanitized_explanation,
                        tool.name = %tool_call.name,
                        tool.request_id = %tool_request.id,
                        tool.call_json = %tool_call_json,
                        "{}",
                        if above_threshold {
                            "prompt injection scan: finding above threshold"
                        } else {
                            "prompt injection scan: finding below threshold"
                        }
                    );
                    if above_threshold {
                        results.push(SecurityResult {
                            is_malicious: analysis_result.is_malicious,
                            confidence: analysis_result.confidence,
                            explanation: analysis_result.explanation,
                            should_ask_user: true, // Always ask user for threats above threshold
                            finding_id,
                            tool_request_id: tool_request.id.clone(),
                        });
                    }
                } else if analysis_result.scanned {
                    let tool_call_json = redacted_tool_call_json(tool_call);

                    tracing::info!(
                        monotonic_counter.goose.prompt_injection_tool_call_passed = 1,
                        security.event_type = "prompt_injection_scan",
                        security.action = "ALLOW",
                        security.confidence = analysis_result.confidence,
                        security.threshold = config_threshold,
                        security.above_threshold = false,
                        security.threat_type = "command_injection",
                        tool.name = %tool_call.name,
                        tool.request_id = %tool_request.id,
                        tool.call_json = %tool_call_json,
                        "prompt injection scan: tool call passed"
                    );
                }
            }
        }

        tracing::info!(
            monotonic_counter.goose.prompt_injection_analysis_performed = 1,
            security_issues_found = results.len(),
            "Prompt injection detection: Security analysis complete"
        );
        Ok(results)
    }

    pub async fn filter_malicious_tool_calls(
        &self,
        messages: &[Message],
        permission_check_result: &PermissionCheckResult,
        _system_prompt: Option<&str>,
    ) -> Result<Vec<SecurityResult>> {
        let tool_requests: Vec<_> = permission_check_result
            .approved
            .iter()
            .chain(permission_check_result.needs_approval.iter())
            .cloned()
            .collect();

        self.analyze_tool_requests(&tool_requests, messages).await
    }
}

impl Default for SecurityManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::CallToolRequestParams;
    use rmcp::object;

    #[test]
    fn redacted_tool_call_json_redacts_high_entropy_secrets() {
        let tool_call = CallToolRequestParams::new("shell").with_arguments(object!({
            "command": "curl -H 'Authorization: Bearer sk-ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789' https://example.com"
        }));

        let json = redacted_tool_call_json(&tool_call);

        assert!(
            !json.contains("sk-ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"),
            "logged tool_call_json should not contain the raw secret: {json}"
        );
        assert!(
            json.contains("[REDACTED]"),
            "expected a redaction marker: {json}"
        );
        assert!(
            json.contains("shell"),
            "tool name should still be visible for audit: {json}"
        );
    }

    #[test]
    fn redacted_tool_call_json_leaves_ordinary_arguments_intact() {
        let tool_call = CallToolRequestParams::new("developer__text_editor")
            .with_arguments(object!({"path": "/tmp/example.txt", "command": "view"}));

        let json = redacted_tool_call_json(&tool_call);

        assert!(json.contains("/tmp/example.txt"));
        assert!(!json.contains("[REDACTED]"));
    }

    #[test]
    fn redacted_tool_call_json_redacts_secret_before_a_stray_colon_space() {
        let tool_call = CallToolRequestParams::new("shell").with_arguments(object!({
            "api_key": "AKIAABCDEFGHIJKLMNOP1234567890ABCDEF",
            "command": "run: fetch RANDOMSECRETVALUEXYZ987654321 done"
        }));

        let json = redacted_tool_call_json(&tool_call);

        assert!(
            !json.contains("AKIAABCDEFGHIJKLMNOP1234567890ABCDEF"),
            "a secret positioned before a stray ': ' must still be redacted: {json}"
        );
        assert!(
            json.contains("[REDACTED]"),
            "expected a redaction marker: {json}"
        );
    }

    #[test]
    fn redacted_tool_call_json_redacts_bearer_header_value() {
        let tool_call = CallToolRequestParams::new("http_request").with_arguments(object!({
            "Authorization": "Bearer sk-live-ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
        }));

        let json = redacted_tool_call_json(&tool_call);

        assert!(
            !json.contains("sk-live-ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"),
            "a 'Bearer <token>' header value must be redacted: {json}"
        );
        assert!(
            json.contains("[REDACTED]"),
            "expected a redaction marker: {json}"
        );
    }
}
