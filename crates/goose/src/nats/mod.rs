//! Opt-in native NATS publishing of goose events.
//!
//! When `GOOSE_NATS_URL` is set, goose publishes a best-effort event firehose
//! (one subject per session + event type) to a NATS bus for fleet observability
//! and agent-to-agent buses. It is:
//!   - **opt-in**: a no-op unless `GOOSE_NATS_URL` is configured;
//!   - **non-blocking**: the agent path only ever `try_send`s onto a bounded
//!     channel and drops on overflow, so a slow/dead broker never stalls a turn;
//!   - **startup-safe**: connects in the background with retry, so a missing
//!     broker never fails startup;
//!   - **best-effort** (Core NATS, at-most-once) — telemetry, not a transactional log.

use crate::config::Config;
use crate::conversation::message::{Message, MessageContent};
use bytes::Bytes;
use std::sync::OnceLock;
use tokio::sync::mpsc;

const DEFAULT_SUBJECT_PREFIX: &str = "goose";
const CHANNEL_CAPACITY: usize = 10_000;
/// Cap published text so a large tool output never produces an oversized message.
const MAX_TEXT_BYTES: usize = 8 * 1024;

/// A single event to publish. Subject is `{prefix}.{session}.{kind}`.
struct NatsEvent {
    session_id: String,
    kind: &'static str,
    payload: serde_json::Value,
}

/// Cheap, cloneable handle. The agent holds one and only ever calls `emit`.
pub struct NatsPublisher {
    tx: mpsc::Sender<NatsEvent>,
}

impl NatsPublisher {
    /// Hot-path call: synchronous, non-blocking, drop-on-full.
    fn emit(&self, event: NatsEvent) {
        let _ = self.tx.try_send(event);
    }
}

static PUBLISHER: OnceLock<Option<NatsPublisher>> = OnceLock::new();

/// Returns the process-global publisher, initializing it on first use from
/// config. Returns `None` when `GOOSE_NATS_URL` is unset (feature off).
fn publisher() -> Option<&'static NatsPublisher> {
    PUBLISHER.get_or_init(init_from_config).as_ref()
}

fn init_from_config() -> Option<NatsPublisher> {
    let config = Config::global();
    let url: String = config.get_param("GOOSE_NATS_URL").ok()?;
    if url.trim().is_empty() {
        return None;
    }
    let subject_prefix: String = config
        .get_param("GOOSE_NATS_SUBJECT")
        .ok()
        .filter(|s: &String| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SUBJECT_PREFIX.to_string());

    let (tx, rx) = mpsc::channel::<NatsEvent>(CHANNEL_CAPACITY);
    tokio::spawn(run_publisher(url, subject_prefix.clone(), rx));
    tracing::info!("NATS publishing enabled (prefix '{}')", subject_prefix);
    Some(NatsPublisher { tx })
}

async fn run_publisher(url: String, subject_prefix: String, mut rx: mpsc::Receiver<NatsEvent>) {
    let client = match async_nats::ConnectOptions::new()
        .name("goose")
        .retry_on_initial_connect()
        .max_reconnects(None)
        .event_callback(|event| async move {
            tracing::debug!("nats connection event: {:?}", event);
        })
        .connect(&url)
        .await
    {
        Ok(client) => client,
        Err(e) => {
            tracing::warn!("NATS disabled: connect to {} failed: {}", url, e);
            return;
        }
    };

    let prefix_for_subject = |session: &str, kind: &str| {
        format!("{}.{}.{}", subject_prefix, sanitize_token(session), kind)
    };

    while let Some(event) = rx.recv().await {
        let envelope = serde_json::json!({
            "v": 1,
            "type": event.kind,
            "ts": chrono::Utc::now().to_rfc3339(),
            "session_id": event.session_id,
            "payload": event.payload,
        });
        let body = match serde_json::to_vec(&envelope) {
            Ok(b) => Bytes::from(b),
            Err(_) => continue,
        };
        let subject = prefix_for_subject(&event.session_id, event.kind);
        if let Err(e) = client.publish(subject, body).await {
            tracing::debug!("nats publish failed: {}", e);
        }
    }
    let _ = client.drain().await;
}

/// NATS subjects are dot-delimited; strip anything that would create extra
/// tokens or wildcards so a session id maps to exactly one token.
fn sanitize_token(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
            _ => '_',
        })
        .collect()
}

/// Publish a conversation message event. No-op unless NATS is configured.
pub fn publish_message(session_id: &str, message: &Message) {
    let Some(pubr) = publisher() else { return };

    let kind = if message.is_tool_call() {
        "tool.requested"
    } else if message.is_tool_response() {
        "tool.responded"
    } else if message.role == rmcp::model::Role::User {
        "message.user"
    } else {
        "message.assistant"
    };

    let mut text: String = message.as_concat_text();
    if text.len() > MAX_TEXT_BYTES {
        // Truncate on a char boundary.
        let mut end = MAX_TEXT_BYTES;
        while end > 0 && !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str("…[truncated]");
    }
    let content_kinds: Vec<&'static str> = message.content.iter().map(content_kind).collect();

    let payload = serde_json::json!({
        "role": format!("{:?}", message.role).to_lowercase(),
        "text": text,
        "content_kinds": content_kinds,
    });

    pubr.emit(NatsEvent {
        session_id: session_id.to_string(),
        kind,
        payload,
    });
}

fn content_kind(c: &MessageContent) -> &'static str {
    match c {
        MessageContent::Text(_) => "text",
        MessageContent::Image(_) => "image",
        MessageContent::ToolRequest(_) => "tool_request",
        MessageContent::ToolResponse(_) => "tool_response",
        MessageContent::ToolConfirmationRequest(_) => "tool_confirmation_request",
        MessageContent::ActionRequired(_) => "action_required",
        MessageContent::FrontendToolRequest(_) => "frontend_tool_request",
        MessageContent::Thinking(_) => "thinking",
        MessageContent::RedactedThinking(_) => "redacted_thinking",
        MessageContent::SystemNotification(_) => "system_notification",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_token_strips_dots_and_wildcards() {
        assert_eq!(sanitize_token("a.b*c>d e"), "a_b_c_d_e");
        assert_eq!(sanitize_token("019ef2e3-59"), "019ef2e3-59");
    }

    #[test]
    fn publish_message_is_noop_when_unconfigured() {
        // With GOOSE_NATS_URL unset in the test env, this must not panic.
        let msg = Message::user().with_text("hello");
        publish_message("test-session", &msg);
    }
}
