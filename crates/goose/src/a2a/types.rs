//! A2A (Agent2Agent) protocol types — JSON-RPC v0.3 binding.
//!
//! Field names, the `kind` discriminator, lowercase enum strings, and the
//! `/.well-known/agent-card.json` path follow the A2A v0.3.0 JSON Schema
//! (`a2aproject/A2A` `specification/json/a2a.json`), which is the form the
//! reference SDKs (a2a-js v0.3, a2a-python's v0.3 compat adapter) interoperate
//! with today.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Well-known path for the Agent Card (current SDK constant).
pub const AGENT_CARD_WELL_KNOWN_PATH: &str = "/.well-known/agent-card.json";
/// Legacy well-known path, served too for older clients.
pub const AGENT_CARD_LEGACY_PATH: &str = "/.well-known/agent.json";
pub const A2A_PROTOCOL_VERSION: &str = "0.3";
pub const TRANSPORT_JSONRPC: &str = "JSONRPC";

// ---------------------------------------------------------------------------
// Agent Card
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCard {
    pub protocol_version: String,
    pub name: String,
    pub description: String,
    pub url: String,
    pub version: String,
    pub capabilities: AgentCapabilities,
    pub default_input_modes: Vec<String>,
    pub default_output_modes: Vec<String>,
    pub skills: Vec<AgentSkill>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_transport: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<AgentProvider>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streaming: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub push_notifications: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_transition_history: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_modes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_modes: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProvider {
    pub organization: String,
    pub url: String,
}

// ---------------------------------------------------------------------------
// Message / Part
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Agent,
}

/// A2A message part, discriminated by `kind`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Part {
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<Value>,
    },
    File {
        file: FileContent,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<Value>,
    },
    Data {
        data: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<Value>,
    },
}

impl Part {
    pub fn text(text: impl Into<String>) -> Self {
        Part::Text {
            text: text.into(),
            metadata: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    /// const "message"
    pub kind: String,
    pub message_id: String,
    pub role: Role,
    pub parts: Vec<Part>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

impl Message {
    pub fn agent_text(message_id: String, text: String, context_id: Option<String>) -> Self {
        Message {
            kind: "message".to_string(),
            message_id,
            role: Role::Agent,
            parts: vec![Part::text(text)],
            context_id,
            task_id: None,
            metadata: None,
        }
    }

    /// Concatenate all text parts (file/data parts are ignored).
    pub fn concat_text(&self) -> String {
        let mut out = String::new();
        for p in &self.parts {
            if let Part::Text { text, .. } = p {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(text);
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Task
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskState {
    Submitted,
    Working,
    InputRequired,
    Completed,
    Canceled,
    Failed,
    Rejected,
    AuthRequired,
    Unknown,
}

impl TaskState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            TaskState::Completed | TaskState::Canceled | TaskState::Failed | TaskState::Rejected
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatus {
    pub state: TaskState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

impl TaskStatus {
    pub fn new(state: TaskState) -> Self {
        TaskStatus {
            state,
            message: None,
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    /// const "task"
    pub kind: String,
    pub id: String,
    pub context_id: String,
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<Vec<Artifact>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<Message>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub artifact_id: String,
    pub parts: Vec<Part>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// ---------------------------------------------------------------------------
// Method params
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct MessageSendParams {
    pub message: Message,
    #[serde(default)]
    pub configuration: Option<Value>,
    #[serde(default)]
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskQueryParams {
    pub id: String,
    #[serde(default)]
    pub history_length: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TaskIdParams {
    pub id: String,
}

// ---------------------------------------------------------------------------
// JSON-RPC envelope
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    pub fn ok(id: Value, result: Value) -> Self {
        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }
    pub fn err(id: Value, error: JsonRpcError) -> Self {
        JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        JsonRpcError {
            code,
            message: message.into(),
            data: None,
        }
    }
    // Standard JSON-RPC codes.
    pub fn parse_error() -> Self {
        Self::new(-32700, "Invalid JSON payload")
    }
    pub fn invalid_request() -> Self {
        Self::new(-32600, "Invalid Request")
    }
    pub fn method_not_found(method: &str) -> Self {
        Self::new(-32601, format!("Method not found: {method}"))
    }
    pub fn invalid_params(msg: impl Into<String>) -> Self {
        Self::new(-32602, msg)
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(-32603, msg)
    }
    // A2A-specific codes (a2a.json).
    pub fn task_not_found() -> Self {
        Self::new(-32001, "Task not found")
    }
    pub fn task_not_cancelable() -> Self {
        Self::new(-32002, "Task cannot be canceled")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_text_round_trips_with_kind_discriminator() {
        let p = Part::text("hello");
        let j = serde_json::to_value(&p).unwrap();
        assert_eq!(j["kind"], "text");
        assert_eq!(j["text"], "hello");
        let back: Part = serde_json::from_value(j).unwrap();
        matches!(back, Part::Text { .. });
    }

    #[test]
    fn task_state_serializes_kebab_lowercase() {
        assert_eq!(
            serde_json::to_value(TaskState::InputRequired).unwrap(),
            "input-required"
        );
        assert_eq!(
            serde_json::to_value(TaskState::Completed).unwrap(),
            "completed"
        );
        let s: TaskState = serde_json::from_value(serde_json::json!("auth-required")).unwrap();
        assert_eq!(s, TaskState::AuthRequired);
    }

    #[test]
    fn incoming_user_message_deserializes() {
        let j = serde_json::json!({
            "kind": "message",
            "role": "user",
            "messageId": "m1",
            "parts": [{"kind": "text", "text": "hi"}]
        });
        let m: Message = serde_json::from_value(j).unwrap();
        assert_eq!(m.role, Role::User);
        assert_eq!(m.concat_text(), "hi");
    }

    #[test]
    fn agent_card_uses_camel_case_and_required_fields() {
        let card = AgentCard {
            protocol_version: A2A_PROTOCOL_VERSION.to_string(),
            name: "goose".into(),
            description: "d".into(),
            url: "http://x/a2a".into(),
            version: "1".into(),
            capabilities: AgentCapabilities {
                streaming: Some(false),
                ..Default::default()
            },
            default_input_modes: vec!["text/plain".into()],
            default_output_modes: vec!["text/plain".into()],
            skills: vec![],
            preferred_transport: Some(TRANSPORT_JSONRPC.into()),
            provider: None,
            documentation_url: None,
        };
        let j = serde_json::to_value(&card).unwrap();
        assert_eq!(j["protocolVersion"], "0.3");
        assert_eq!(j["defaultInputModes"][0], "text/plain");
        assert_eq!(j["preferredTransport"], "JSONRPC");
    }

    #[test]
    fn task_serializes_with_kind_task() {
        let t = Task {
            kind: "task".into(),
            id: "t1".into(),
            context_id: "c1".into(),
            status: TaskStatus::new(TaskState::Completed),
            artifacts: None,
            history: None,
            metadata: None,
        };
        let j = serde_json::to_value(&t).unwrap();
        assert_eq!(j["kind"], "task");
        assert_eq!(j["contextId"], "c1");
        assert_eq!(j["status"]["state"], "completed");
    }
}
