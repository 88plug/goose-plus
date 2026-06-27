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
    AgentCapabilities, AgentCard, AgentInterface, AgentProvider, AgentSkill,
    HttpAuthSecurityScheme, Message as A2aMessage, PartContent, Role, SecurityRequirement,
    SecurityScheme, TRANSPORT_PROTOCOL_HTTP_JSON, TRANSPORT_PROTOCOL_JSONRPC,
    TRANSPORT_PROTOCOL_WEBSOCKET,
};
use base64::Engine;
use std::collections::HashMap;

/// Name of the HTTP bearer security scheme advertised on the Agent Card when a
/// token is configured. Clients reference it from the security requirement.
const BEARER_SCHEME_NAME: &str = "bearer";

/// Build goose's Agent Card. `origin` is the externally reachable HTTP origin
/// (e.g. `http://host:port`); the JSON-RPC, REST, and WebSocket interfaces are
/// derived from it to match how [`crate::a2a`]'s server router mounts them.
///
/// When `auth_token` is `Some`, the card advertises an HTTP bearer security
/// scheme so clients know to send `Authorization: Bearer <token>`. When `None`,
/// the security fields stay empty (open access).
pub fn build_agent_card(origin: &str, version: &str, auth_token: Option<&str>) -> AgentCard {
    let origin = origin.trim_end_matches('/');
    let ws_origin = origin
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    let (security_schemes, security_requirements) = if auth_token.is_some() {
        let mut schemes = HashMap::new();
        schemes.insert(
            BEARER_SCHEME_NAME.to_string(),
            SecurityScheme::HttpAuth(HttpAuthSecurityScheme {
                scheme: "bearer".to_string(),
                description: Some("Bearer token configured via GOOSE_A2A_TOKEN.".to_string()),
                bearer_format: None,
            }),
        );
        let mut requirement: SecurityRequirement = HashMap::new();
        requirement.insert(BEARER_SCHEME_NAME.to_string(), vec![]);
        (Some(schemes), Some(vec![requirement]))
    } else {
        (None, None)
    };
    AgentCard {
        name: "goose-plus".to_string(),
        description:
            "goose-plus — an enhanced open-source AI agent for code, workflows, and everything \
             in between."
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
        skills: vec![
            AgentSkill {
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
            },
            // Dedicated skill for the parallel free SearXNG superpower.
            // This advertises that goose can be called directly for fast, smart search.
            AgentSkill {
                id: "searxng_parallel_search".to_string(),
                name: "SearXNG Parallel Search".to_string(),
                description:
                    "Privacy-first metasearch: one query is fanned out to all 8 verified free \
                     public SearXNG providers in full parallel (no concurrency limit). Fast-fail \
                     on bad backends, automatic HTML fallback, smart merge (dedup by URL + \
                     engine aggregation + hit boosting). Incremental results are streamed back \
                     immediately over A2A (Working updates) for lowest time-to-first-result. \
                     This is the secret sauce making searxng-mcp the most powerful search \
                     surface for agents."
                        .to_string(),
                tags: vec![
                    "search".to_string(),
                    "metasearch".to_string(),
                    "privacy".to_string(),
                    "parallel".to_string(),
                ],
                examples: Some(vec![
                    "searxng: best open source ai agent frameworks 2026".to_string(),
                    "latest rust web frameworks".to_string(),
                ]),
                input_modes: None,
                output_modes: None,
                security_requirements: None,
            },
        ],
        provider: Some(AgentProvider {
            organization: "goose-plus".to_string(),
            url: "https://github.com/88plug/goose-plus".to_string(),
        }),
        documentation_url: Some("https://github.com/88plug/goose-plus".to_string()),
        icon_url: None,
        security_schemes,
        security_requirements,
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
        let card = build_agent_card("http://localhost:3000", "1.0.0", None);
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
    fn agent_card_omits_security_without_token() {
        let card = build_agent_card("http://localhost:3000", "1.0.0", None);
        assert!(card.security_schemes.is_none());
        assert!(card.security_requirements.is_none());
    }

    #[test]
    fn agent_card_advertises_bearer_with_token() {
        let card = build_agent_card("http://localhost:3000", "1.0.0", Some("secret"));
        let schemes = card.security_schemes.expect("schemes advertised");
        assert!(matches!(
            schemes.get(BEARER_SCHEME_NAME),
            Some(SecurityScheme::HttpAuth(s)) if s.scheme == "bearer"
        ));
        let reqs = card.security_requirements.expect("requirements advertised");
        assert!(reqs.iter().any(|r| r.contains_key(BEARER_SCHEME_NAME)));
    }

    #[test]
    fn a2a_text_message_converts_to_goose_user_text() {
        let m = A2aMessage::new(Role::User, vec![Part::text("hello goose")]);
        let g = a2a_message_to_goose(&m);
        assert_eq!(g.role, rmcp::model::Role::User);
        assert_eq!(goose_text(&g), "hello goose");
    }
}
