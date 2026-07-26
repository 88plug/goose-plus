//! A2A (Agent2Agent) server.
//!
//! Built from our fork's routers ([`a2a_server`] JSON-RPC + REST, plus
//! [`a2a_websocket`]). The only goose-specific code is [`GooseExecutor`], which
//! bridges an inbound A2A message to `agent.reply(...)` and streams the result
//! back as A2A events.
//!
//! Opt-in (mounted only when `GOOSE_A2A_ENABLE` is set) and deliberately not
//! behind goose's internal `x-secret-key` middleware: A2A clients authenticate
//! per the Agent Card's own scheme. When `GOOSE_A2A_TOKEN` is set, the
//! rpc/rest/ws routes require a constant-time `Authorization: Bearer <token>`
//! and the card advertises that bearer scheme; the `.well-known` agent card
//! stays public for discovery. With no token configured, the routes are open.

use a2a::{
    event::StreamResponse, A2AError, Message as A2aMessage, Part, PartContent, Role, Task,
    TaskState, TaskStatus, TaskStatusUpdateEvent,
};
use a2a_server::{
    agent_card::agent_card_router, executor::ExecutorContext, jsonrpc::jsonrpc_router,
    rest::rest_router, AgentExecutor, DefaultRequestHandler, InMemoryTaskStore, StaticAgentCard,
};
use a2a_websocket::websocket_router;
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
    Router,
};
use futures::stream::BoxStream;
use futures::StreamExt;
use goose::acp::transport::auth::token_matches;
use goose::agents::{types::SessionConfig, AgentEvent};
use goose_mcp::{SearchUpdate, SearxngServer};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::AbortHandle;
use tokio_stream::wrappers::ReceiverStream;

use crate::state::AppState;

/// Extract concatenated text from an A2A message (for intent detection).
fn extract_text(msg: &A2aMessage) -> String {
    msg.parts
        .iter()
        .filter_map(|p| {
            if let PartContent::Text(t) = &p.content {
                Some(t.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Detect an explicit searxng direct-search query.
/// Supports prefixes like "searxng: query", "searx query", "search: query".
/// Returns the query string if matched.
fn parse_searxng_query(text: &str) -> Option<String> {
    let t = text.trim();
    // Match the prefix ASCII-case-insensitively, then slice the query out of
    // the original text so proper nouns and acronyms reach SearXNG intact.
    // Comparing raw bytes also guarantees the split lands on a char boundary.
    for prefix in ["searxng:", "searx ", "search:"] {
        let Some(head) = t.as_bytes().get(..prefix.len()) else {
            continue;
        };
        if head.eq_ignore_ascii_case(prefix.as_bytes()) {
            let q = t.get(prefix.len()..).unwrap_or_default().trim();
            if !q.is_empty() {
                return Some(q.to_string());
            }
        }
    }
    None
}

/// Format a partial (non-final) SearchUpdate for A2A Working status.
fn format_searxng_partial(update: &SearchUpdate, query: &str) -> String {
    let mut lines = vec![
        format!(
            "SearXNG parallel ({} providers) — partial from {}",
            update.total_merged, update.backend
        ),
        format!("Query: {}", query),
    ];
    for (i, r) in update.merged_preview.iter().take(5).enumerate() {
        let title = r.title.as_deref().unwrap_or(&r.url);
        lines.push(format!("{}. {} — {}", i + 1, title, r.url));
    }
    if update.merged_preview.len() > 5 {
        lines.push(format!("... +{} more", update.merged_preview.len() - 5));
    }
    lines.join("\n")
}

/// Format the final merged results (similar shape to the MCP tool output).
fn format_searxng_final(update: &SearchUpdate, query: &str) -> String {
    if update.merged_preview.is_empty() {
        return format!("No results for '{}'", query);
    }
    let mut lines = vec![
        format!("Query: {}", query),
        format!("Free providers (full parallel, no limit): 8"),
        "".to_string(),
    ];
    for (i, r) in update.merged_preview.iter().enumerate() {
        let title = r.title.as_deref().unwrap_or(&r.url);
        lines.push(format!(
            "{}. {} — {} {}",
            i + 1,
            title,
            r.url,
            if !r.engines.is_empty() {
                format!("(via {})", r.engines.join(","))
            } else {
                "".into()
            }
        ));
        if let Some(sn) = &r.snippet {
            lines.push(format!("   {}", sn));
        }
    }
    lines.join("\n")
}

/// Bridges A2A execution to goose's agent. Each `execute` drives one
/// `agent.reply` to completion, emitting a `Working` status, streaming the
/// agent's text deltas as `Working` updates, then a terminal `Completed` task
/// carrying the full text.
struct GooseExecutor {
    app: Arc<AppState>,
    tasks: Arc<Mutex<HashMap<String, AbortHandle>>>,
}

impl AgentExecutor for GooseExecutor {
    fn execute(
        &self,
        ctx: ExecutorContext,
    ) -> BoxStream<'static, Result<StreamResponse, A2AError>> {
        let app = self.app.clone();
        let tasks = self.tasks.clone();
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let task_key = ctx.task_id.clone();
        let handle = tokio::spawn(async move {
            let task_id = ctx.task_id.clone();
            let context_id = ctx.context_id.clone();

            let _ = tx.send(Ok(working(&task_id, &context_id))).await;

            let Some(a2a_msg) = ctx.message else {
                let _ = tx
                    .send(Ok(completed(&task_id, &context_id, String::new())))
                    .await;
                return;
            };

            let msg_text = extract_text(&a2a_msg);

            // === Direct fast path for searxng_parallel_search skill / prefixed queries ===
            // This bypasses a full LLM turn and streams partial merged results immediately.
            // One query → all 8 free providers in full parallel → incremental results over A2A.
            if let Some(q) = parse_searxng_query(&msg_text) {
                // Indicate we are starting the parallel free search
                let _ = tx
                    .send(Ok(working_with_text(
                        &task_id,
                        &context_id,
                        &format!("Searching 8 free SearXNG providers in full parallel (no limit) for: {}", q),
                    )))
                    .await;

                let searx = SearxngServer::new();
                let mut stream = searx.parallel_search_stream(&q, "en").await;

                while let Some(update) = stream.next().await {
                    if update.is_final {
                        let final_text = format_searxng_final(&update, &q);
                        let _ = tx
                            .send(Ok(completed(&task_id, &context_id, final_text)))
                            .await;
                    } else {
                        let partial = format_searxng_partial(&update, &q);
                        let _ = tx
                            .send(Ok(working_with_text(&task_id, &context_id, &partial)))
                            .await;
                    }
                }
                return; // fast path done — no full agent turn
            }

            let user_message = goose::a2a::a2a_message_to_goose(&a2a_msg);

            let agent = match app.get_agent(context_id.clone()).await {
                Ok(a) => a,
                Err(e) => {
                    let _ = tx.send(Err(A2AError::internal(e.to_string()))).await;
                    return;
                }
            };

            // Map the A2A contextId onto a goose session row before replying so
            // persisted messages satisfy the messages -> sessions foreign key.
            let working_dir =
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            if let Err(e) = app
                .session_manager()
                .ensure_session(&context_id, working_dir)
                .await
            {
                let _ = tx.send(Err(A2AError::internal(e.to_string()))).await;
                return;
            }

            // A2A is a headless entry point: if no provider has been configured
            // on this agent, bootstrap one from GOOSE_PROVIDER / GOOSE_MODEL.
            if agent.provider().await.is_err() {
                let cfg = goose::config::Config::global();
                let init = match (cfg.get_goose_provider(), cfg.get_goose_model()) {
                    (Ok(provider_name), Ok(model)) => {
                        async {
                            let model_config = goose::model_config::model_config_from_user_config(
                                &provider_name,
                                &model,
                            )?;
                            let extensions = goose::session::EnabledExtensionsState::for_session(
                                app.session_manager(),
                                &context_id,
                                cfg,
                            )
                            .await;
                            let provider =
                                goose::providers::create(&provider_name, model_config, extensions)
                                    .await?;
                            agent.update_provider(provider, &context_id).await?;
                            anyhow::Ok(())
                        }
                        .await
                    }
                    _ => Err(anyhow::anyhow!(
                        "no provider configured (set GOOSE_PROVIDER and GOOSE_MODEL)"
                    )),
                };
                if let Err(e) = init {
                    let _ = tx.send(Err(A2AError::internal(e.to_string()))).await;
                    return;
                }
            }

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
                        // agent.reply streams incremental text deltas as separate
                        // messages; concatenate them directly (the model's own
                        // text carries its newlines).
                        let delta = goose::a2a::goose_text(&m);
                        if !delta.is_empty() {
                            let _ = tx
                                .send(Ok(working_with_text(&task_id, &context_id, &delta)))
                                .await;
                            agent_text.push_str(&delta);
                        }
                    }
                }
            }

            let _ = tx
                .send(Ok(completed(&task_id, &context_id, agent_text)))
                .await;
        });
        // Register the spawned turn so cancel() can abort it mid-stream. The
        // task removes its own entry once it finishes (below).
        {
            let tasks = tasks.clone();
            let abort = handle.abort_handle();
            let key = task_key.clone();
            tokio::spawn(async move {
                tasks.lock().await.insert(key.clone(), abort);
                let _ = handle.await;
                tasks.lock().await.remove(&key);
            });
        }
        Box::pin(ReceiverStream::new(rx))
    }

    fn cancel(&self, ctx: ExecutorContext) -> BoxStream<'static, Result<StreamResponse, A2AError>> {
        let tasks = self.tasks.clone();
        let task_id = ctx.task_id;
        let context_id = ctx.context_id;
        Box::pin(futures::stream::once(async move {
            // Abort the in-flight turn, which drops the agent.reply stream and
            // ends the spawned task. Removing the entry here is best-effort; the
            // task's own cleanup also removes it.
            if let Some(abort) = tasks.lock().await.remove(&task_id) {
                abort.abort();
            }
            Ok(StreamResponse::StatusUpdate(TaskStatusUpdateEvent {
                task_id,
                context_id,
                status: TaskStatus {
                    state: TaskState::Canceled,
                    message: None,
                    timestamp: Some(chrono::Utc::now()),
                },
                metadata: None,
            }))
        }))
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

fn working_with_text(task_id: &str, context_id: &str, text: &str) -> StreamResponse {
    let mut agent_msg = A2aMessage::new(Role::Agent, vec![Part::text(text)]);
    agent_msg.task_id = Some(task_id.to_string());
    agent_msg.context_id = Some(context_id.to_string());
    StreamResponse::StatusUpdate(TaskStatusUpdateEvent {
        task_id: task_id.to_string(),
        context_id: context_id.to_string(),
        status: TaskStatus {
            state: TaskState::Working,
            message: Some(agent_msg),
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

/// Require `Authorization: Bearer <token>`, comparing constant-time.
async fn check_a2a_bearer(
    State(expected): State<String>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let presented = request
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    if token_matches(presented, &expected) {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

/// Build the A2A router. `origin` is the externally reachable HTTP origin
/// (e.g. `http://host:port`); the Agent Card derives its interface URLs from it.
///
/// When `GOOSE_A2A_TOKEN` is set and non-empty, the rpc/rest/ws routers require
/// a matching `Authorization: Bearer` header and the Agent Card advertises the
/// bearer scheme; the public agent-card discovery route stays open. When unset,
/// everything mounts open, as before.
pub fn router(app: Arc<AppState>, origin: String) -> Router {
    let token = goose::config::Config::global()
        .get_text_param("GOOSE_A2A_TOKEN")
        .filter(|t| !t.trim().is_empty());

    let handler = Arc::new(DefaultRequestHandler::new(
        GooseExecutor {
            app,
            tasks: Default::default(),
        },
        InMemoryTaskStore::new(),
    ));
    let card = goose::a2a::build_agent_card(&origin, env!("CARGO_PKG_VERSION"), token.as_deref());
    let card_producer = Arc::new(StaticAgentCard::new(card));

    let mut protected = Router::new()
        .nest("/jsonrpc", jsonrpc_router(handler.clone()))
        .nest("/rest", rest_router(handler.clone()))
        .nest("/a2a/ws", websocket_router(handler));

    if let Some(token) = token {
        protected = protected.layer(axum::middleware::from_fn_with_state(
            token,
            check_a2a_bearer,
        ));
    }

    protected.merge(agent_card_router(card_producer))
}
