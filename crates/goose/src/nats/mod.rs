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
use futures::StreamExt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use tokio::sync::mpsc;

const DEFAULT_SUBJECT_PREFIX: &str = "goose";
const CHANNEL_CAPACITY: usize = 10_000;
/// Cap published text so a large tool output never produces an oversized message.
const MAX_TEXT_BYTES: usize = 8 * 1024;
/// Queue group so multiple goose instances share inbound drive work.
const DRIVE_QUEUE_GROUP: &str = "goose-drive";

/// Process-global monotonic sequence stamped on every published envelope.
static SEQ: AtomicU64 = AtomicU64::new(0);
/// Count of events dropped because the publish channel was full.
static DROPPED: AtomicU64 = AtomicU64::new(0);

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
        if self.tx.try_send(event).is_err() {
            let total = DROPPED.fetch_add(1, Ordering::Relaxed) + 1;
            tracing::debug!(
                "nats event dropped (channel full); total dropped: {}",
                total
            );
        }
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

    let instance = instance_identity(config);

    let (tx, rx) = mpsc::channel::<NatsEvent>(CHANNEL_CAPACITY);
    tokio::spawn(run_publisher(
        url,
        subject_prefix.clone(),
        instance.clone(),
        rx,
    ));
    tracing::info!(
        "NATS publishing enabled (prefix '{}', instance '{}')",
        subject_prefix,
        instance
    );
    Some(NatsPublisher { tx })
}

/// Stable identity for this process: `GOOSE_NATS_INSTANCE` when set, else
/// `{hostname}:{pid}`. Avoids a hostname crate by reading `$HOSTNAME`.
fn instance_identity(config: &Config) -> String {
    if let Ok(Some(name)) = config.get_param::<Option<String>>("GOOSE_NATS_INSTANCE") {
        if !name.trim().is_empty() {
            return name;
        }
    }
    let host = std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string());
    format!("{}:{}", host, std::process::id())
}

async fn run_publisher(
    url: String,
    subject_prefix: String,
    instance: String,
    mut rx: mpsc::Receiver<NatsEvent>,
) {
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
            "seq": SEQ.fetch_add(1, Ordering::Relaxed),
            "instance": instance,
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

/// Publish an arbitrary event. No-op unless NATS is configured.
pub fn publish_event(session_id: &str, kind: &'static str, payload: serde_json::Value) {
    let Some(pubr) = publisher() else { return };
    pubr.emit(NatsEvent {
        session_id: session_id.to_string(),
        kind,
        payload,
    });
}

/// Publish a conversation message event. No-op unless NATS is configured.
pub fn publish_message(session_id: &str, message: &Message) {
    if publisher().is_none() {
        return;
    }

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

    publish_event(session_id, kind, payload);
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

/// Inbound command parsed from a `{prefix}.cmd` message.
#[derive(serde::Deserialize)]
struct DriveCommand {
    session_id: Option<String>,
    prompt: String,
}

/// Subscribe to `{prefix}.cmd` and let NATS DRIVE goose: each command runs the
/// injected `handler` (which bridges to `agent.reply`) and the reply is
/// published back to the message's reply subject (or `{prefix}.{session}.reply`).
///
/// Opt-in: the caller only invokes this when `GOOSE_NATS_DRIVE` is enabled, but
/// it is also defensive — returns immediately if `GOOSE_NATS_URL` is unset.
/// Runs forever (until the process exits or the connection permanently closes).
pub async fn run_drive_loop<H, F>(handler: H)
where
    H: Fn(String, String) -> F + Send + Sync + 'static,
    F: std::future::Future<Output = anyhow::Result<String>> + Send + 'static,
{
    let config = Config::global();
    let Ok(url) = config.get_param::<String>("GOOSE_NATS_URL") else {
        return;
    };
    if url.trim().is_empty() {
        return;
    }
    let subject_prefix: String = config
        .get_param("GOOSE_NATS_SUBJECT")
        .ok()
        .filter(|s: &String| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SUBJECT_PREFIX.to_string());

    let client = match async_nats::ConnectOptions::new()
        .name("goose")
        .retry_on_initial_connect()
        .max_reconnects(None)
        .connect(&url)
        .await
    {
        Ok(client) => client,
        Err(e) => {
            tracing::warn!("NATS drive disabled: connect to {} failed: {}", url, e);
            return;
        }
    };

    let cmd_subject = format!("{}.cmd", subject_prefix);
    let mut subscription = match client
        .queue_subscribe(cmd_subject.clone(), DRIVE_QUEUE_GROUP.to_string())
        .await
    {
        Ok(sub) => sub,
        Err(e) => {
            tracing::warn!(
                "NATS drive disabled: subscribe to {} failed: {}",
                cmd_subject,
                e
            );
            return;
        }
    };

    tracing::info!(
        "NATS drive loop listening on '{}' (queue '{}')",
        cmd_subject,
        DRIVE_QUEUE_GROUP
    );

    let handler = Arc::new(handler);
    while let Some(msg) = subscription.next().await {
        let cmd: DriveCommand = match serde_json::from_slice(&msg.payload) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("nats drive: invalid command payload: {}", e);
                continue;
            }
        };

        let session_id = match cmd.session_id.filter(|s| !s.trim().is_empty()) {
            Some(s) => s,
            None => match &msg.reply {
                Some(reply) => sanitize_token(reply.as_str()),
                None => {
                    tracing::warn!(
                        "nats drive: command missing session_id and reply subject; skipping"
                    );
                    continue;
                }
            },
        };

        let handler = handler.clone();
        let client = client.clone();
        let prefix = subject_prefix.clone();
        let reply_subject = msg.reply.clone();
        let prompt = cmd.prompt;
        tokio::spawn(async move {
            let envelope = match handler(session_id.clone(), prompt).await {
                Ok(text) => serde_json::json!({
                    "v": 1,
                    "type": "drive.reply",
                    "session_id": session_id,
                    "payload": { "text": text },
                }),
                Err(e) => serde_json::json!({
                    "v": 1,
                    "type": "drive.error",
                    "session_id": session_id,
                    "payload": { "error": e.to_string() },
                }),
            };
            let body = match serde_json::to_vec(&envelope) {
                Ok(b) => Bytes::from(b),
                Err(e) => {
                    tracing::debug!("nats drive: serialize reply failed: {}", e);
                    return;
                }
            };
            let subject = reply_subject
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{}.{}.reply", prefix, sanitize_token(&session_id)));
            if let Err(e) = client.publish(subject, body).await {
                tracing::debug!("nats drive: publish reply failed: {}", e);
            }
        });
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

    #[test]
    fn emit_on_full_channel_increments_dropped() {
        let (tx, rx) = mpsc::channel::<NatsEvent>(1);
        drop(rx);
        let publisher = NatsPublisher { tx };

        let before = DROPPED.load(Ordering::Relaxed);
        publisher.emit(NatsEvent {
            session_id: "s".to_string(),
            kind: "test",
            payload: serde_json::json!({}),
        });
        let after = DROPPED.load(Ordering::Relaxed);

        assert_eq!(after, before + 1);
    }
}
