//! Minimal outbound A2A client: discover a remote agent and send it a message.
//!
//! Used by the `a2a__call_remote_agent` platform tool so goose can delegate to
//! other A2A agents. Speaks the v0.3 JSON-RPC `message/send` method.

use super::types::AgentCard;
use anyhow::{anyhow, Result};
use serde_json::{json, Value};

pub struct A2aClient {
    http: reqwest::Client,
}

impl Default for A2aClient {
    fn default() -> Self {
        Self::new()
    }
}

impl A2aClient {
    pub fn new() -> Self {
        A2aClient {
            http: reqwest::Client::new(),
        }
    }

    /// Fetch the Agent Card from `{base}/.well-known/agent-card.json`, falling
    /// back to the legacy `/.well-known/agent.json` path.
    pub async fn fetch_agent_card(&self, base: &str) -> Result<AgentCard> {
        let base = base.trim_end_matches('/');
        let primary = format!("{base}{}", super::types::AGENT_CARD_WELL_KNOWN_PATH);
        match self.try_fetch_card(&primary).await {
            Ok(card) => Ok(card),
            Err(_) => {
                let legacy = format!("{base}{}", super::types::AGENT_CARD_LEGACY_PATH);
                self.try_fetch_card(&legacy).await
            }
        }
    }

    async fn try_fetch_card(&self, url: &str) -> Result<AgentCard> {
        let card = self
            .http
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json::<AgentCard>()
            .await?;
        Ok(card)
    }

    /// Discover the agent at `agent_base`, send `text` via `message/send`, and
    /// return the agent's reply text. Blocking semantics (no streaming).
    pub async fn send_text(&self, agent_base: &str, text: &str) -> Result<String> {
        let card = self.fetch_agent_card(agent_base).await?;
        let rpc_url = card.url;

        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "message/send",
            "params": {
                "message": {
                    "kind": "message",
                    "role": "user",
                    "messageId": uuid::Uuid::now_v7().to_string(),
                    "parts": [{ "kind": "text", "text": text }]
                }
            }
        });

        let resp: Value = self
            .http
            .post(&rpc_url)
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        if let Some(err) = resp.get("error") {
            return Err(anyhow!("remote A2A agent returned error: {err}"));
        }
        let result = resp
            .get("result")
            .ok_or_else(|| anyhow!("A2A response missing result"))?;
        Ok(extract_reply_text(result))
    }
}

/// Extract human-readable text from a `message/send` result, which is a `Task`
/// or a `Message` (a2a.json). Prefers, in order: a `Message` result's text; a
/// Task's `status.message`; the last agent message in `history`; artifact text.
fn extract_reply_text(result: &Value) -> String {
    let kind = result.get("kind").and_then(|k| k.as_str());

    if kind == Some("message") {
        return parts_text(result.get("parts"));
    }

    // Task
    if let Some(msg) = result.get("status").and_then(|s| s.get("message")) {
        let t = parts_text(msg.get("parts"));
        if !t.is_empty() {
            return t;
        }
    }
    if let Some(history) = result.get("history").and_then(|h| h.as_array()) {
        if let Some(last_agent) = history
            .iter()
            .rev()
            .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("agent"))
        {
            let t = parts_text(last_agent.get("parts"));
            if !t.is_empty() {
                return t;
            }
        }
    }
    if let Some(artifacts) = result.get("artifacts").and_then(|a| a.as_array()) {
        let mut out = String::new();
        for art in artifacts {
            let t = parts_text(art.get("parts"));
            if !t.is_empty() {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&t);
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    // Fall back to the task state so the caller learns *something* happened.
    let state = result
        .get("status")
        .and_then(|s| s.get("state"))
        .and_then(|s| s.as_str())
        .unwrap_or("unknown");
    format!("(remote agent returned a task in state '{state}' with no text)")
}

fn parts_text(parts: Option<&Value>) -> String {
    let Some(arr) = parts.and_then(|p| p.as_array()) else {
        return String::new();
    };
    let mut out = String::new();
    for p in arr {
        if p.get("kind").and_then(|k| k.as_str()) == Some("text") {
            if let Some(t) = p.get("text").and_then(|t| t.as_str()) {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(t);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_text_from_message_result() {
        let r = json!({"kind":"message","role":"agent","parts":[{"kind":"text","text":"hi back"}]});
        assert_eq!(extract_reply_text(&r), "hi back");
    }

    #[test]
    fn extracts_text_from_task_status_message() {
        let r = json!({
            "kind":"task","id":"t","contextId":"c",
            "status":{"state":"completed","message":{"kind":"message","role":"agent","parts":[{"kind":"text","text":"done"}]}}
        });
        assert_eq!(extract_reply_text(&r), "done");
    }

    #[test]
    fn extracts_text_from_task_history() {
        let r = json!({
            "kind":"task","id":"t","contextId":"c","status":{"state":"completed"},
            "history":[
                {"role":"user","parts":[{"kind":"text","text":"q"}]},
                {"role":"agent","parts":[{"kind":"text","text":"a"}]}
            ]
        });
        assert_eq!(extract_reply_text(&r), "a");
    }

    #[test]
    fn falls_back_to_state_when_no_text() {
        let r = json!({"kind":"task","id":"t","contextId":"c","status":{"state":"working"}});
        assert!(extract_reply_text(&r).contains("working"));
    }
}
