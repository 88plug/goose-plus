//! Outbound A2A client: discover a remote agent and send it a message.
//!
//! Used by the `a2a__call_remote_agent` platform tool so goose can delegate to
//! other A2A agents. Built on our fork's [`a2a_client`] factory, which resolves
//! the remote Agent Card and negotiates the best transport — JSON-RPC and REST
//! by default, plus WebSocket which we register explicitly.

use a2a::{Message, Part, Role, SendMessageRequest, SendMessageResponse};
use a2a_client::{agent_card::AgentCardResolver, A2AClientFactory};
use a2a_websocket::{WebSocketTransportFactory, TRANSPORT_PROTOCOL_WEBSOCKET};
use anyhow::{anyhow, Result};
use std::sync::Arc;

#[derive(Default)]
pub struct A2aClient;

impl A2aClient {
    pub fn new() -> Self {
        A2aClient
    }

    /// Discover the agent at `agent_base`, send `text`, and return its reply
    /// text. Blocking semantics (`message:send`, no streaming).
    pub async fn send_text(&self, agent_base: &str, text: &str) -> Result<String> {
        let card = AgentCardResolver::new(None)
            .resolve(agent_base)
            .await
            .map_err(|e| anyhow!("failed to resolve A2A agent card: {e}"))?;

        let factory = A2AClientFactory::builder()
            .register(Arc::new(WebSocketTransportFactory))
            .preferred_bindings(vec![
                a2a::TRANSPORT_PROTOCOL_JSONRPC.to_string(),
                a2a::TRANSPORT_PROTOCOL_HTTP_JSON.to_string(),
                TRANSPORT_PROTOCOL_WEBSOCKET.to_string(),
            ])
            .build();

        let client = factory
            .create_from_card(&card)
            .await
            .map_err(|e| anyhow!("no compatible A2A transport: {e}"))?;

        let request = SendMessageRequest {
            message: Message::new(Role::User, vec![Part::text(text)]),
            configuration: None,
            metadata: None,
            tenant: None,
        };

        let response = client
            .send_message(&request)
            .await
            .map_err(|e| anyhow!("remote A2A agent error: {e}"))?;

        Ok(extract_reply_text(response))
    }
}

/// Pull human-readable text out of a `message:send` response (a `Message` or a
/// terminal `Task`).
fn extract_reply_text(response: SendMessageResponse) -> String {
    match response {
        SendMessageResponse::Message(m) => parts_text(&m),
        SendMessageResponse::Task(task) => {
            if let Some(msg) = task.status.message.as_ref() {
                let t = parts_text(msg);
                if !t.is_empty() {
                    return t;
                }
            }
            if let Some(last_agent) = task
                .history
                .as_ref()
                .and_then(|h| h.iter().rev().find(|m| m.role == Role::Agent))
            {
                let t = parts_text(last_agent);
                if !t.is_empty() {
                    return t;
                }
            }
            format!(
                "(remote agent returned a task in state {:?} with no text)",
                task.status.state
            )
        }
    }
}

fn parts_text(msg: &Message) -> String {
    msg.parts
        .iter()
        .filter_map(|p| p.as_text())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_text_from_message_response() {
        let resp =
            SendMessageResponse::Message(Message::new(Role::Agent, vec![Part::text("hi back")]));
        assert_eq!(extract_reply_text(resp), "hi back");
    }
}
