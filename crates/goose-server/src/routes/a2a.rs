//! A2A (Agent2Agent) server.
//!
//! Built from our fork's routers ([`a2a_server`] JSON-RPC + REST, plus
//! [`a2a_websocket`]). The only goose-specific code is [`GooseExecutor`], which
//! bridges an inbound A2A message to `agent.reply(...)` and streams the result
//! back as A2A events.
//!
//! Opt-in (mounted only when `GOOSE_A2A_ENABLE` is set) and intentionally
//! mounted WITHOUT the `x-secret-key` middleware, since remote A2A clients
//! authenticate per the Agent Card's own scheme, not goose's internal secret.

use a2a::{
    event::StreamResponse, A2AError, Message as A2aMessage, Part, Role, Task, TaskState,
    TaskStatus, TaskStatusUpdateEvent,
};
use a2a_server::{
    agent_card::agent_card_router, executor::ExecutorContext, jsonrpc::jsonrpc_router,
    rest::rest_router, AgentExecutor, DefaultRequestHandler, InMemoryTaskStore, StaticAgentCard,
};
use a2a_websocket::websocket_router;
use axum::Router;
use futures::stream::BoxStream;
use futures::StreamExt;
use goose::agents::{types::SessionConfig, AgentEvent};
use std::sync::Arc;
use tokio_stream::wrappers::ReceiverStream;

use crate::state::AppState;

/// Bridges A2A execution to goose's agent. Each `execute` drives one
/// `agent.reply` to completion, emitting a `Working` status then a terminal
/// `Completed` task carrying the agent's text.
struct GooseExecutor {
    app: Arc<AppState>,
}

impl AgentExecutor for GooseExecutor {
    fn execute(
        &self,
        ctx: ExecutorContext,
    ) -> BoxStream<'static, Result<StreamResponse, A2AError>> {
        let app = self.app.clone();
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move {
            let task_id = ctx.task_id.clone();
            let context_id = ctx.context_id.clone();

            let _ = tx.send(Ok(working(&task_id, &context_id))).await;

            let Some(a2a_msg) = ctx.message else {
                let _ = tx
                    .send(Ok(completed(&task_id, &context_id, String::new())))
                    .await;
                return;
            };

            let user_message = goose::a2a::a2a_message_to_goose(&a2a_msg);

            let agent = match app.get_agent(context_id.clone()).await {
                Ok(a) => a,
                Err(e) => {
                    let _ = tx.send(Err(A2AError::internal(e.to_string()))).await;
                    return;
                }
            };

            let session_config = SessionConfig {
                id: context_id.clone(),
                schedule_id: None,
                max_turns: Some(50),
                retry_config: None,
            };

            let mut stream = match agent.reply(user_message, session_config, None).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(Err(A2AError::internal(e.to_string()))).await;
                    return;
                }
            };

            let mut agent_text = String::new();
            while let Some(event) = stream.next().await {
                if let Ok(AgentEvent::Message(m)) = event {
                    if m.role == rmcp::model::Role::Assistant {
                        let t = goose::a2a::goose_text(&m);
                        if !t.is_empty() {
                            if !agent_text.is_empty() {
                                agent_text.push('\n');
                            }
                            agent_text.push_str(&t);
                        }
                    }
                }
            }

            let _ = tx
                .send(Ok(completed(&task_id, &context_id, agent_text)))
                .await;
        });
        Box::pin(ReceiverStream::new(rx))
    }

    fn cancel(&self, ctx: ExecutorContext) -> BoxStream<'static, Result<StreamResponse, A2AError>> {
        let event = StreamResponse::StatusUpdate(TaskStatusUpdateEvent {
            task_id: ctx.task_id,
            context_id: ctx.context_id,
            status: TaskStatus {
                state: TaskState::Canceled,
                message: None,
                timestamp: Some(chrono::Utc::now()),
            },
            metadata: None,
        });
        Box::pin(futures::stream::once(async move { Ok(event) }))
    }
}

fn working(task_id: &str, context_id: &str) -> StreamResponse {
    StreamResponse::StatusUpdate(TaskStatusUpdateEvent {
        task_id: task_id.to_string(),
        context_id: context_id.to_string(),
        status: TaskStatus {
            state: TaskState::Working,
            message: None,
            timestamp: Some(chrono::Utc::now()),
        },
        metadata: None,
    })
}

fn completed(task_id: &str, context_id: &str, text: String) -> StreamResponse {
    let mut agent_msg = A2aMessage::new(Role::Agent, vec![Part::text(text)]);
    agent_msg.task_id = Some(task_id.to_string());
    agent_msg.context_id = Some(context_id.to_string());
    StreamResponse::Task(Task {
        id: task_id.to_string(),
        context_id: context_id.to_string(),
        status: TaskStatus {
            state: TaskState::Completed,
            message: Some(agent_msg),
            timestamp: Some(chrono::Utc::now()),
        },
        artifacts: None,
        history: None,
        metadata: None,
    })
}

/// Build the A2A router. `origin` is the externally reachable HTTP origin
/// (e.g. `http://host:port`); the Agent Card derives its interface URLs from it.
pub fn router(app: Arc<AppState>, origin: String) -> Router {
    let handler = Arc::new(DefaultRequestHandler::new(
        GooseExecutor { app },
        InMemoryTaskStore::new(),
    ));
    let card = goose::a2a::build_agent_card(&origin, env!("CARGO_PKG_VERSION"));
    let card_producer = Arc::new(StaticAgentCard::new(card));

    Router::new()
        .nest("/jsonrpc", jsonrpc_router(handler.clone()))
        .nest("/rest", rest_router(handler.clone()))
        .nest("/a2a/ws", websocket_router(handler))
        .merge(agent_card_router(card_producer))
}
