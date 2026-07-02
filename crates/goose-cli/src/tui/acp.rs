use agent_client_protocol::schema::{
    ContentBlock, InitializeRequest, ListSessionsRequest, McpServer, ProtocolVersion,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionConfigId, SessionConfigValueId, SessionNotification,
    SessionUpdate, SetSessionConfigOptionRequest, ToolCallContent as AcpToolCallContent,
    ToolCallStatus, ToolKind,
};
use agent_client_protocol::{ActiveSession, Agent, Client, ConnectionTo};
use anyhow::Result;
use goose_sdk::custom_requests::*;
use std::path::PathBuf;
use tokio::sync::mpsc;

const PROVIDER_CONFIG_ID: &str = "provider";
const PROVIDER_MODEL_META_KEY: &str = "model";
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

#[derive(Debug, Clone)]
pub(super) enum AgentMessage {
    TextChunk(String),
    ToolCallStarted {
        title: String,
        id: String,
        kind: ToolKind,
        status: ToolCallStatus,
        raw_input: Option<serde_json::Value>,
        raw_output: Option<serde_json::Value>,
        content: Vec<AcpToolCallContent>,
    },
    ToolCallUpdate {
        id: String,
        title: Option<String>,
        kind: Option<ToolKind>,
        status: Option<ToolCallStatus>,
        raw_input: Option<serde_json::Value>,
        raw_output: Option<serde_json::Value>,
        content: Option<Vec<AcpToolCallContent>>,
    },
    ResponseComplete,
    Error(String),
    SessionCreated,
    DefaultsSaved {
        provider: String,
        model: String,
    },
    ProviderChanged {
        provider: String,
        model: String,
    },
    SessionsList(Vec<SessionInfo>),
    ProvidersList(Vec<ProviderInfo>),
    ProviderModelsList {
        provider: String,
        models: Vec<String>,
    },
    ExtensionsList(Vec<ExtensionInfo>),
    Initialized,
}

#[derive(Debug, Clone)]
pub(super) struct SessionInfo {
    pub(super) title: String,
    pub(super) updated_at: String,
}

#[derive(Debug, Clone)]
pub(super) struct ProviderInfo {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) configured: bool,
    pub(super) description: String,
    pub(super) models: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct ExtensionInfo {
    pub(super) name: String,
    pub(super) enabled: bool,
    pub(super) ext_type: String,
}

#[derive(Debug, Clone)]
pub(super) enum ClientCommand {
    Initialize,
    CreateSession,
    SendPrompt(String),
    ListSessions,
    ListProviders,
    ListExtensions,
    ListProviderModels { provider: String },
    SaveDefaults { provider: String, model: String },
    ToggleExtension { key: String, enabled: bool },
    Shutdown,
}

pub(super) fn spawn_acp_client(
    goose_bin: PathBuf,
) -> (
    mpsc::UnboundedSender<ClientCommand>,
    mpsc::UnboundedReceiver<AgentMessage>,
) {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (msg_tx, msg_rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        if let Err(error) = run_client(goose_bin, cmd_rx, msg_tx.clone()).await {
            let _ = msg_tx.send(AgentMessage::Error(error.to_string()));
        }
    });
    (cmd_tx, msg_rx)
}

async fn run_client(
    goose_bin: PathBuf,
    mut cmd_rx: mpsc::UnboundedReceiver<ClientCommand>,
    msg_tx: mpsc::UnboundedSender<AgentMessage>,
) -> Result<()> {
    let mut child = tokio::process::Command::new(&goose_bin)
        .arg("acp")
        .arg("--with-builtin")
        .arg("developer")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    let child_stdin = child.stdin.take().expect("stdin piped");
    let child_stdout = child.stdout.take().expect("stdout piped");
    let transport =
        agent_client_protocol::ByteStreams::new(child_stdin.compat_write(), child_stdout.compat());
    let notification_tx = msg_tx.clone();
    let permission_tx = msg_tx.clone();

    Client
        .builder()
        .name("goose-tui")
        .on_receive_notification(
            async move |notification: SessionNotification, _cx| {
                handle_notification(&notification, &notification_tx);
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |request: RequestPermissionRequest, responder, _cx| {
                let _ =
                    permission_tx.send(AgentMessage::TextChunk("\nAuto-approving tool\n".into()));
                let response = request
                    .options
                    .first()
                    .map(|option| {
                        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                            option.option_id.clone(),
                        ))
                    })
                    .unwrap_or(RequestPermissionOutcome::Cancelled);
                responder.respond(RequestPermissionResponse::new(response))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(transport, async move |cx| {
            run_command_loop(cx, &mut cmd_rx, &msg_tx).await
        })
        .await?;

    let _ = child.kill().await;
    Ok(())
}

fn handle_notification(
    notification: &SessionNotification,
    msg_tx: &mpsc::UnboundedSender<AgentMessage>,
) {
    match &notification.update {
        SessionUpdate::AgentMessageChunk(chunk) => {
            if let ContentBlock::Text(text) = &chunk.content {
                let _ = msg_tx.send(AgentMessage::TextChunk(text.text.clone()));
            }
        }
        SessionUpdate::ToolCall(tool_call) => {
            let _ = msg_tx.send(AgentMessage::ToolCallStarted {
                title: tool_call.title.clone(),
                id: tool_call.tool_call_id.0.to_string(),
                kind: tool_call.kind,
                status: tool_call.status,
                raw_input: tool_call.raw_input.clone(),
                raw_output: tool_call.raw_output.clone(),
                content: tool_call.content.clone(),
            });
        }
        SessionUpdate::ToolCallUpdate(update) => {
            let _ = msg_tx.send(AgentMessage::ToolCallUpdate {
                id: update.tool_call_id.0.to_string(),
                title: update.fields.title.clone(),
                kind: update.fields.kind,
                status: update.fields.status,
                raw_input: update.fields.raw_input.clone(),
                raw_output: update.fields.raw_output.clone(),
                content: update.fields.content.clone(),
            });
        }
        _ => {}
    }
}

async fn run_command_loop(
    cx: ConnectionTo<agent_client_protocol::Agent>,
    cmd_rx: &mut mpsc::UnboundedReceiver<ClientCommand>,
    msg_tx: &mpsc::UnboundedSender<AgentMessage>,
) -> Result<(), agent_client_protocol::Error> {
    while let Some(cmd) = cmd_rx.recv().await {
        if matches!(cmd, ClientCommand::Initialize) {
            break;
        }
    }

    cx.send_request(InitializeRequest::new(ProtocolVersion::LATEST))
        .block_task()
        .await?;
    let _ = msg_tx.send(AgentMessage::Initialized);

    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            ClientCommand::CreateSession => {
                let _ = msg_tx.send(AgentMessage::SessionCreated);
                cx.build_session_cwd()
                    .map_err(|_| agent_client_protocol::Error::internal_error())?
                    .block_task()
                    .run_until(async |mut session| {
                        while let Some(cmd) = cmd_rx.recv().await {
                            match cmd {
                                ClientCommand::SendPrompt(prompt) => {
                                    if let Err(error) = session.send_prompt(&prompt) {
                                        send_error(msg_tx, "Failed to send prompt", error);
                                        continue;
                                    }
                                    if let Err(error) = session.read_to_string().await {
                                        send_error(msg_tx, "Provider request failed", error);
                                        continue;
                                    }
                                    let _ = msg_tx.send(AgentMessage::ResponseComplete);
                                }
                                ClientCommand::SaveDefaults { provider, model } => {
                                    handle_session_defaults_change(
                                        &mut session,
                                        &provider,
                                        &model,
                                        msg_tx,
                                    )
                                    .await;
                                }
                                ClientCommand::CreateSession | ClientCommand::Shutdown => {
                                    return Ok(());
                                }
                                other => handle_non_session_cmd(&cx, other, msg_tx).await?,
                            }
                        }
                        Ok(())
                    })
                    .await?;
            }
            ClientCommand::Shutdown => break,
            other => handle_non_session_cmd(&cx, other, msg_tx).await?,
        }
    }
    Ok(())
}

fn send_error(
    msg_tx: &mpsc::UnboundedSender<AgentMessage>,
    context: &str,
    error: agent_client_protocol::Error,
) {
    let detail = provider_error_message(context, &error);
    let _ = msg_tx.send(AgentMessage::Error(detail));
}

fn provider_error_message(context: &str, error: &agent_client_protocol::Error) -> String {
    let data = error
        .data
        .as_ref()
        .map(|data| {
            data.as_str().map(ToString::to_string).unwrap_or_else(|| {
                serde_json::to_string_pretty(data).unwrap_or_else(|_| data.to_string())
            })
        })
        .filter(|data| !data.trim().is_empty());

    match data {
        Some(data) if error.message.is_empty() => format!("{context}: {data}"),
        Some(data) => format!("{context}: {}\n{data}", error.message),
        None => format!("{context}: {}", error.message),
    }
}

async fn handle_session_defaults_change(
    session: &mut ActiveSession<'_, Agent>,
    provider: &str,
    model: &str,
    msg_tx: &mpsc::UnboundedSender<AgentMessage>,
) {
    let connection = session.connection();
    let mut meta = serde_json::Map::new();
    meta.insert(
        PROVIDER_MODEL_META_KEY.to_string(),
        serde_json::Value::String(model.to_string()),
    );
    match connection
        .send_request(
            SetSessionConfigOptionRequest::new(
                session.session_id().clone(),
                SessionConfigId::new(PROVIDER_CONFIG_ID),
                SessionConfigValueId::new(provider.to_string()),
            )
            .meta(meta),
        )
        .block_task()
        .await
    {
        Ok(_) => {
            let _ = msg_tx.send(AgentMessage::ProviderChanged {
                provider: provider.to_string(),
                model: model.to_string(),
            });
        }
        Err(error) => send_error(msg_tx, "Failed to change provider", error),
    }
}

async fn handle_non_session_cmd(
    cx: &ConnectionTo<agent_client_protocol::Agent>,
    cmd: ClientCommand,
    msg_tx: &mpsc::UnboundedSender<AgentMessage>,
) -> Result<(), agent_client_protocol::Error> {
    match cmd {
        ClientCommand::ListSessions => {
            let resp = cx
                .send_request(ListSessionsRequest::default())
                .block_task()
                .await?;
            let sessions = resp
                .sessions
                .into_iter()
                .map(|session| SessionInfo {
                    title: session.title.unwrap_or_default(),
                    updated_at: session.updated_at.unwrap_or_default(),
                })
                .collect();
            let _ = msg_tx.send(AgentMessage::SessionsList(sessions));
        }
        ClientCommand::ListProviders => {
            let resp = cx
                .send_request(ListProvidersRequest::default())
                .block_task()
                .await?;
            let providers = resp
                .entries
                .into_iter()
                .map(|entry| ProviderInfo {
                    id: entry.provider_id,
                    name: entry.provider_name,
                    configured: entry.configured,
                    description: entry.description,
                    models: entry.models.into_iter().map(|model| model.id).collect(),
                })
                .collect();
            let _ = msg_tx.send(AgentMessage::ProvidersList(providers));
        }
        ClientCommand::ListExtensions => {
            let resp = cx
                .send_request(GetExtensionsRequest {})
                .block_task()
                .await?;
            let extensions = resp
                .extensions
                .into_iter()
                .map(|entry| {
                    let (name, ext_type) = match &entry.extension {
                        GooseExtension::Builtin { name, .. } => (name.clone(), "builtin"),
                        GooseExtension::Platform { name, .. } => (name.clone(), "platform"),
                        GooseExtension::Mcp { server, .. } => {
                            let name = match server {
                                McpServer::Http(s) => s.name.clone(),
                                McpServer::Sse(s) => s.name.clone(),
                                McpServer::Stdio(s) => s.name.clone(),
                                _ => "unknown".to_string(),
                            };
                            (name, "mcp")
                        }
                    };
                    ExtensionInfo {
                        name,
                        enabled: entry.enabled,
                        ext_type: ext_type.to_string(),
                    }
                })
                .collect();
            let _ = msg_tx.send(AgentMessage::ExtensionsList(extensions));
        }
        ClientCommand::ListProviderModels { provider } => {
            match cx
                .send_request(ProviderSupportedModelsListRequest {
                    provider_id: provider.clone(),
                })
                .block_task()
                .await
            {
                Ok(response) => {
                    let mut models = response.models;
                    models.sort();
                    models.dedup();
                    let _ = msg_tx.send(AgentMessage::ProviderModelsList { provider, models });
                }
                Err(error) => send_error(msg_tx, "Failed to fetch provider models", error),
            }
        }
        ClientCommand::SaveDefaults { provider, model } => {
            match cx
                .send_request(DefaultsSaveRequest {
                    provider_id: provider.clone(),
                    model_id: Some(model.clone()),
                })
                .block_task()
                .await
            {
                Ok(response) => {
                    let _ = msg_tx.send(AgentMessage::DefaultsSaved {
                        provider: response.provider_id.unwrap_or(provider),
                        model: response.model_id.unwrap_or(model),
                    });
                }
                Err(error) => send_error(msg_tx, "Failed to save provider defaults", error),
            }
        }
        ClientCommand::ToggleExtension { key, enabled } => {
            let _ = cx
                .send_request(SetConfigExtensionEnabledRequest {
                    config_key: key,
                    enabled,
                })
                .block_task()
                .await;
        }
        ClientCommand::Initialize
        | ClientCommand::CreateSession
        | ClientCommand::SendPrompt(_)
        | ClientCommand::Shutdown => {}
    }
    Ok(())
}
