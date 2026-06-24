//! A2A (Agent2Agent) server: Agent Card discovery + JSON-RPC v0.3 endpoint.
//!
//! Opt-in (mounted only when `GOOSE_A2A_ENABLE` is set) and intentionally
//! mounted WITHOUT the `x-secret-key` middleware, since remote A2A clients
//! authenticate per the Agent Card's own scheme, not goose's internal secret.
//! Implements the minimal interoperable surface: `message/send` (blocking),
//! `tasks/get`, `tasks/cancel`. Streaming is advertised as unsupported.

use axum::{
    extract::State,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use goose::a2a::{self, types};
use goose::agents::{types::SessionConfig, AgentEvent};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::state::AppState;

#[derive(Clone)]
struct A2aState {
    app: Arc<AppState>,
    /// Externally advertised JSON-RPC endpoint URL (Agent Card `url`).
    base_url: String,
    /// Completed tasks, kept so `tasks/get` works after a blocking send.
    tasks: Arc<Mutex<HashMap<String, types::Task>>>,
}

/// Build the A2A router. `base_url` is the advertised JSON-RPC endpoint.
pub fn router(app: Arc<AppState>, base_url: String) -> Router {
    let state = A2aState {
        app,
        base_url,
        tasks: Arc::new(Mutex::new(HashMap::new())),
    };
    Router::new()
        .route(types::AGENT_CARD_WELL_KNOWN_PATH, get(agent_card))
        .route(types::AGENT_CARD_LEGACY_PATH, get(agent_card))
        .route("/a2a", post(jsonrpc))
        .with_state(state)
}

async fn agent_card(State(state): State<A2aState>) -> Json<types::AgentCard> {
    let version = env!("CARGO_PKG_VERSION");
    Json(a2a::build_agent_card(&state.base_url, version))
}

/// JSON-RPC 2.0 dispatch. Always returns HTTP 200 with a JSON-RPC envelope.
async fn jsonrpc(State(state): State<A2aState>, body: Json<Value>) -> impl IntoResponse {
    let req: types::JsonRpcRequest = match serde_json::from_value(body.0) {
        Ok(r) => r,
        Err(_) => {
            return Json(types::JsonRpcResponse::err(
                Value::Null,
                types::JsonRpcError::parse_error(),
            ));
        }
    };
    if req.jsonrpc != "2.0" {
        return Json(types::JsonRpcResponse::err(
            req.id,
            types::JsonRpcError::invalid_request(),
        ));
    }

    let id = req.id.clone();
    let result = match req.method.as_str() {
        "message/send" => handle_message_send(&state, req.params).await,
        "tasks/get" => handle_tasks_get(&state, req.params),
        "tasks/cancel" => handle_tasks_cancel(&state, req.params),
        other => Err(types::JsonRpcError::method_not_found(other)),
    };

    match result {
        Ok(value) => Json(types::JsonRpcResponse::ok(id, value)),
        Err(e) => Json(types::JsonRpcResponse::err(id, e)),
    }
}

async fn handle_message_send(
    state: &A2aState,
    params: Value,
) -> Result<Value, types::JsonRpcError> {
    let params: types::MessageSendParams = serde_json::from_value(params)
        .map_err(|e| types::JsonRpcError::invalid_params(e.to_string()))?;

    let context_id = params
        .message
        .context_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());

    let user_message = a2a::a2a_message_to_goose(&params.message);

    let agent = state
        .app
        .get_agent(context_id.clone())
        .await
        .map_err(|e| types::JsonRpcError::internal(e.to_string()))?;

    let session_config = SessionConfig {
        id: context_id.clone(),
        schedule_id: None,
        max_turns: Some(50),
        retry_config: None,
    };

    let mut stream = agent
        .reply(user_message, session_config, None)
        .await
        .map_err(|e| types::JsonRpcError::internal(e.to_string()))?;

    use futures::StreamExt;
    let mut agent_text = String::new();
    while let Some(event) = stream.next().await {
        if let Ok(AgentEvent::Message(m)) = event {
            if m.role == rmcp::model::Role::Assistant {
                let t = a2a::goose_text(&m);
                if !t.is_empty() {
                    if !agent_text.is_empty() {
                        agent_text.push('\n');
                    }
                    agent_text.push_str(&t);
                }
            }
        }
    }

    let agent_msg = types::Message::agent_text(
        uuid::Uuid::now_v7().to_string(),
        agent_text,
        Some(context_id.clone()),
    );
    let task_id = uuid::Uuid::now_v7().to_string();
    let task = types::Task {
        kind: "task".to_string(),
        id: task_id.clone(),
        context_id,
        status: types::TaskStatus {
            state: types::TaskState::Completed,
            message: Some(agent_msg.clone()),
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
        },
        artifacts: None,
        history: Some(vec![params.message, agent_msg]),
        metadata: None,
    };

    if let Ok(mut tasks) = state.tasks.lock() {
        tasks.insert(task_id, task.clone());
    }
    serde_json::to_value(task).map_err(|e| types::JsonRpcError::internal(e.to_string()))
}

fn handle_tasks_get(state: &A2aState, params: Value) -> Result<Value, types::JsonRpcError> {
    let params: types::TaskQueryParams = serde_json::from_value(params)
        .map_err(|e| types::JsonRpcError::invalid_params(e.to_string()))?;
    let tasks = state
        .tasks
        .lock()
        .map_err(|_| types::JsonRpcError::internal("task store poisoned"))?;
    match tasks.get(&params.id) {
        Some(task) => {
            serde_json::to_value(task).map_err(|e| types::JsonRpcError::internal(e.to_string()))
        }
        None => Err(types::JsonRpcError::task_not_found()),
    }
}

fn handle_tasks_cancel(state: &A2aState, params: Value) -> Result<Value, types::JsonRpcError> {
    let params: types::TaskIdParams = serde_json::from_value(params)
        .map_err(|e| types::JsonRpcError::invalid_params(e.to_string()))?;
    let tasks = state
        .tasks
        .lock()
        .map_err(|_| types::JsonRpcError::internal("task store poisoned"))?;
    match tasks.get(&params.id) {
        // Tasks complete synchronously, so a known task is always terminal.
        Some(_) => Err(types::JsonRpcError::task_not_cancelable()),
        None => Err(types::JsonRpcError::task_not_found()),
    }
}
