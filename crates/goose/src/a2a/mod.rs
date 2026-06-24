//! A2A (Agent2Agent) protocol support.
//!
//! Built entirely on our fork crate [`a2a`] (88plug/a2a-rs, 88plug-plus — A2A
//! v1.0 + WebSocket transport + spec fixes). This module is only the irreducible
//! bridge between A2A's data model and goose's: the Agent Card describing goose,
//! and the message conversion. The HTTP/WebSocket server (built from the crate's
//! routers) lives in `goose-server` (`routes::a2a`); the outbound client wrapper
//! lives in [`client`].

pub mod client;

use crate::conversation::message::{Message as GooseMessage, MessageContent};
use a2a::{
    AgentCapabilities, AgentCard, AgentInterface, AgentProvider, AgentSkill, Message as A2aMessage,
    PartContent, Role, TRANSPORT_PROTOCOL_HTTP_JSON, TRANSPORT_PROTOCOL_JSONRPC,
    TRANSPORT_PROTOCOL_WEBSOCKET,
};
use base64::Engine;

/// Build goose's Agent Card. `origin` is the externally reachable HTTP origin
/// (e.g. `http://host:port`); the JSON-RPC, REST, and WebSocket interfaces are
/// derived from it to match how [`crate::a2a`]'s server router mounts them.
pub fn build_agent_card(origin: &str, version: &str) -> AgentCard {
    let origin = origin.trim_end_matches('/');
    let ws_origin = origin
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    AgentCard {
        name: "goose".to_string(),
        description:
            "goose — an open-source AI agent for code, workflows, and everything in between."
                .to_string(),
        version: version.to_string(),
        supported_interfaces: vec![
            AgentInterface::new(format!("{origin}/jsonrpc"), TRANSPORT_PROTOCOL_JSONRPC),
            AgentInterface::new(format!("{origin}/rest"), TRANSPORT_PROTOCOL_HTTP_JSON),
            AgentInterface::new(format!("{ws_origin}/a2a/ws"), TRANSPORT_PROTOCOL_WEBSOCKET),
        ],
        capabilities: AgentCapabilities {
            streaming: Some(true),
            push_notifications: Some(false),
            extensions: None,
            extended_agent_card: None,
        },
        default_input_modes: vec!["text/plain".to_string()],
        default_output_modes: vec!["text/plain".to_string()],
        skills: vec![AgentSkill {
            id: "general".to_string(),
            name: "General assistant".to_string(),
            description:
                "Delegate a coding or workflow task to goose; it can read/write files, run \
                 commands, and use its configured MCP extensions."
                    .to_string(),
            tags: vec![
                "code".to_string(),
                "workflow".to_string(),
                "agent".to_string(),
            ],
            examples: Some(vec![
                "Summarize the README of this project".to_string(),
                "Fix the failing test in src/lib.rs".to_string(),
            ]),
            input_modes: None,
            output_modes: None,
            security_requirements: None,
        }],
        provider: Some(AgentProvider {
            organization: "goose".to_string(),
            url: "https://block.github.io/goose/".to_string(),
        }),
        documentation_url: Some("https://block.github.io/goose/".to_string()),
        icon_url: None,
        security_schemes: None,
        security_requirements: None,
        signatures: None,
    }
}

/// Convert an inbound A2A message into a goose user message.
///
/// Text parts map directly. Raw image parts map to a goose image; other raw,
/// url, and data parts are flattened to a textual note (goose has no native
/// binary/file/structured message content).
pub fn a2a_message_to_goose(msg: &A2aMessage) -> GooseMessage {
    let mut goose = match msg.role {
        Role::Agent => GooseMessage::assistant(),
        Role::User | Role::Unspecified => GooseMessage::user(),
    };
    for part in &msg.parts {
        match &part.content {
            PartContent::Text(text) => {
                goose = goose.with_text(text);
            }
            PartContent::Raw(bytes) => {
                let is_image = part
                    .media_type
                    .as_deref()
                    .map(|m| m.starts_with("image/"))
                    .unwrap_or(false);
                if is_image {
                    let mime = part.media_type.clone().unwrap_or_default();
                    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
                    goose = goose.with_image(b64, mime);
                } else {
                    let name = part.filename.as_deref().unwrap_or("file");
                    goose = goose.with_text(format!("[attached binary: {name}]"));
                }
            }
            PartContent::Url(url) => {
                let name = part.filename.as_deref().unwrap_or("file");
                goose = goose.with_text(format!("[attached file: {name} ({url})]"));
            }
            PartContent::Data(data) => {
                goose = goose.with_text(format!("[structured data]\n{data}"));
            }
        }
    }
    goose
}

/// Extract concatenated assistant text from a goose message for an A2A reply.
pub fn goose_text(msg: &GooseMessage) -> String {
    let mut out = String::new();
    for c in &msg.content {
        if let MessageContent::Text(t) = c {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&t.text);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2a::Part;

    #[test]
    fn agent_card_advertises_v1_interfaces() {
        let card = build_agent_card("http://localhost:3000", "1.0.0");
        assert_eq!(card.version, "1.0.0");
        let bindings: Vec<&str> = card
            .supported_interfaces
            .iter()
            .map(|i| i.protocol_binding.as_str())
            .collect();
        assert!(bindings.contains(&TRANSPORT_PROTOCOL_JSONRPC));
        assert!(bindings.contains(&TRANSPORT_PROTOCOL_WEBSOCKET));
        assert!(!card.skills.is_empty());
        assert_eq!(card.capabilities.streaming, Some(true));
    }

    #[test]
    fn a2a_text_message_converts_to_goose_user_text() {
        let m = A2aMessage::new(Role::User, vec![Part::text("hello goose")]);
        let g = a2a_message_to_goose(&m);
        assert_eq!(g.role, rmcp::model::Role::User);
        assert_eq!(goose_text(&g), "hello goose");
    }
}
