//! A2A (Agent2Agent) protocol support.
//!
//! - [`types`] — the v0.3 JSON-RPC wire types.
//! - This module — the Agent Card builder and A2A <-> goose message conversion.
//!
//! The HTTP server (Agent Card endpoint + JSON-RPC handler) lives in
//! `goose-server` (`routes::a2a`); the outbound client lives in [`client`].

pub mod client;
pub mod types;

use crate::conversation::message::{Message as GooseMessage, MessageContent};
use types::{AgentCapabilities, AgentCard, AgentSkill, Message as A2aMessage, Part, Role};

/// Build goose's Agent Card. `base_url` is the externally reachable JSON-RPC
/// endpoint (e.g. `http://host:port/a2a`).
pub fn build_agent_card(base_url: &str, version: &str) -> AgentCard {
    AgentCard {
        protocol_version: types::A2A_PROTOCOL_VERSION.to_string(),
        name: "goose".to_string(),
        description:
            "goose — an open-source AI agent for code, workflows, and everything in between."
                .to_string(),
        url: base_url.to_string(),
        version: version.to_string(),
        capabilities: AgentCapabilities {
            // Streaming (message/stream) is not yet implemented; advertise honestly.
            streaming: Some(false),
            push_notifications: Some(false),
            state_transition_history: Some(false),
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
        }],
        preferred_transport: Some(types::TRANSPORT_JSONRPC.to_string()),
        provider: None,
        documentation_url: Some("https://block.github.io/goose/".to_string()),
    }
}

/// Convert an inbound A2A message into a goose user message.
///
/// Text parts map directly. File parts carrying image bytes map to an image;
/// other file/data parts are flattened to a textual note (goose has no native
/// File/Data message content — see the A2A research notes).
pub fn a2a_message_to_goose(msg: &A2aMessage) -> GooseMessage {
    let mut goose = match msg.role {
        Role::User => GooseMessage::user(),
        Role::Agent => GooseMessage::assistant(),
    };
    for part in &msg.parts {
        match part {
            Part::Text { text, .. } => {
                goose = goose.with_text(text);
            }
            Part::File { file, .. } => {
                let is_image = file
                    .mime_type
                    .as_deref()
                    .map(|m| m.starts_with("image/"))
                    .unwrap_or(false);
                match (&file.bytes, is_image) {
                    (Some(bytes), true) => {
                        let mime = file.mime_type.clone().unwrap_or_default();
                        goose = goose.with_image(bytes.clone(), mime);
                    }
                    _ => {
                        let name = file.name.as_deref().unwrap_or("file");
                        let loc = file.uri.as_deref().unwrap_or("inline");
                        goose = goose.with_text(format!("[attached file: {name} ({loc})]"));
                    }
                }
            }
            Part::Data { data, .. } => {
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

    #[test]
    fn agent_card_is_valid_v03() {
        let card = build_agent_card("http://localhost:3000/a2a", "1.0.0");
        let j = serde_json::to_value(&card).unwrap();
        assert_eq!(j["protocolVersion"], "0.3");
        assert_eq!(j["url"], "http://localhost:3000/a2a");
        assert!(!j["skills"].as_array().unwrap().is_empty());
        assert_eq!(j["capabilities"]["streaming"], false);
    }

    #[test]
    fn a2a_text_message_converts_to_goose_user_text() {
        let m = A2aMessage {
            kind: "message".into(),
            message_id: "m1".into(),
            role: Role::User,
            parts: vec![Part::text("hello goose")],
            context_id: None,
            task_id: None,
            metadata: None,
        };
        let g = a2a_message_to_goose(&m);
        assert_eq!(g.role, rmcp::model::Role::User);
        assert_eq!(goose_text(&g), "hello goose");
    }
}
