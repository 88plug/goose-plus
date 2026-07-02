use agent_client_protocol::schema::{
    ToolCallContent as AcpToolCallContent, ToolCallStatus, ToolKind,
};
use anyhow::Result;
use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event as CrosstermEvent, EventStream, MouseEvent,
    MouseEventKind,
};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::stdout;
use std::time::Duration;
use tokio::sync::mpsc;

mod acp;
mod chat;
mod keyboard;
mod markdown;
mod slash;
mod style;
mod views;

use acp::{
    spawn_acp_client, AgentMessage, ClientCommand, ExtensionInfo, ProviderInfo, SessionInfo,
};
use views::render;

#[derive(Clone, PartialEq)]
enum View {
    Splash,
    Providers,
    Models,
    Chat,
    Sessions,
    Extensions,
}

#[derive(Clone)]
enum TimelineItem {
    Message { role: Role, content: String },
    ToolCall(ToolCall),
    Notice(Notice),
}

#[derive(Clone)]
enum Role {
    User,
    Assistant,
    System,
}

#[derive(Clone)]
struct ToolCall {
    title: String,
    id: String,
    kind: ToolKind,
    status: ToolCallStatus,
    raw_input: Option<serde_json::Value>,
    raw_output: Option<serde_json::Value>,
    content: Vec<AcpToolCallContent>,
}

#[derive(Clone)]
struct Notice {
    title: String,
    body: String,
    kind: NoticeKind,
}

#[derive(Clone)]
enum NoticeKind {
    Info,
    Error,
}

struct App {
    view: View,
    tick: usize,
    timeline: Vec<TimelineItem>,
    input: String,
    cursor: usize,
    streaming: String,
    loading: bool,
    status: String,
    providers: Vec<ProviderInfo>,
    provider_search: String,
    providers_selected: usize,
    models: Vec<String>,
    model_search: String,
    models_selected: usize,
    pending_provider: Option<String>,
    sessions: Vec<SessionInfo>,
    sessions_selected: usize,
    extensions: Vec<ExtensionInfo>,
    extensions_selected: usize,
    selected_tool_call: Option<usize>,
    expanded_tool_call: bool,
    scrollback: usize,
    expanded_scroll: usize,
    show_help_menu: bool,
    slash_selected: usize,
    cmd_tx: mpsc::UnboundedSender<ClientCommand>,
    msg_rx: mpsc::UnboundedReceiver<AgentMessage>,
    should_quit: bool,
    has_session: bool,
}

impl App {
    fn new(
        cmd_tx: mpsc::UnboundedSender<ClientCommand>,
        msg_rx: mpsc::UnboundedReceiver<AgentMessage>,
    ) -> Self {
        Self {
            view: View::Splash,
            tick: 0,
            timeline: Vec::new(),
            input: String::new(),
            cursor: 0,
            streaming: String::new(),
            loading: true,
            status: "starting".into(),
            providers: Vec::new(),
            provider_search: String::new(),
            providers_selected: 0,
            models: Vec::new(),
            model_search: String::new(),
            models_selected: 0,
            pending_provider: None,
            sessions: Vec::new(),
            sessions_selected: 0,
            extensions: Vec::new(),
            extensions_selected: 0,
            selected_tool_call: None,
            expanded_tool_call: false,
            scrollback: 0,
            expanded_scroll: 0,
            show_help_menu: false,
            slash_selected: 0,
            cmd_tx,
            msg_rx,
            should_quit: false,
            has_session: false,
        }
    }

    fn handle_agent_message(&mut self, msg: AgentMessage) {
        match msg {
            AgentMessage::Initialized => {
                self.status = "loading providers".into();
                let _ = self.cmd_tx.send(ClientCommand::ListProviders);
            }
            AgentMessage::ProvidersList(providers) => {
                let has_configured = providers.iter().any(|p| p.configured);
                self.providers = providers;
                self.models.clear();
                self.providers_selected = 0;
                if has_configured && self.view == View::Splash {
                    self.start_session();
                } else {
                    self.status = "choose provider".into();
                    self.loading = false;
                    self.view = View::Providers;
                }
            }
            AgentMessage::SessionCreated => {
                self.has_session = true;
                self.loading = false;
                self.status = "ready".into();
                if self.view != View::Splash {
                    self.view = View::Chat;
                    if self.timeline.is_empty() {
                        self.push_message(Role::System, "What would you like to work on?".into());
                    }
                }
            }
            AgentMessage::DefaultsSaved { provider, model } => {
                self.loading = false;
                self.status = "ready".into();
                self.push_notice(
                    NoticeKind::Info,
                    "Provider defaults updated".into(),
                    format!("New sessions will use {provider} with {model}"),
                );
                self.view = View::Chat;
                if !self.has_session {
                    self.start_session();
                }
            }
            AgentMessage::ProviderChanged { provider, model } => {
                self.loading = false;
                self.status = "ready".into();
                self.push_notice(
                    NoticeKind::Info,
                    "Provider changed".into(),
                    format!("Now using {provider} with {model}"),
                );
            }
            AgentMessage::ProviderModelsList { provider, models } => {
                self.loading = false;
                self.status = "choose model".into();
                self.pending_provider = Some(provider.clone());
                self.models = models;
                self.model_search.clear();
                self.models_selected = 0;
                if self.models.is_empty() {
                    self.push_notice(
                        NoticeKind::Error,
                        "No models found".into(),
                        format!("{provider} did not return any supported models."),
                    );
                    self.view = View::Chat;
                } else {
                    self.view = View::Models;
                }
            }
            AgentMessage::TextChunk(text) => {
                self.loading = true;
                self.status = "thinking".into();
                self.streaming.push_str(&text);
            }
            AgentMessage::ToolCallStarted {
                title,
                id,
                kind,
                status,
                raw_input,
                raw_output,
                content,
            } => {
                self.flush_streaming();
                self.loading = true;
                self.status = "using tools".into();
                self.timeline.push(TimelineItem::ToolCall(ToolCall {
                    title,
                    id,
                    kind,
                    status,
                    raw_input,
                    raw_output,
                    content,
                }));
                self.scrollback = 0;
                if self.selected_tool_call.is_none() {
                    self.selected_tool_call = self.tool_call_count().checked_sub(1);
                }
            }
            AgentMessage::ToolCallUpdate {
                id,
                title,
                kind,
                status,
                raw_input,
                raw_output,
                content,
            } => {
                if let Some(tool) = self.timeline.iter_mut().find_map(|item| match item {
                    TimelineItem::ToolCall(tool) if tool.id == id => Some(tool),
                    _ => None,
                }) {
                    if let Some(title) = title {
                        tool.title = title;
                    }
                    if let Some(kind) = kind {
                        tool.kind = kind;
                    }
                    if let Some(status) = status {
                        tool.status = status;
                    }
                    if raw_input.is_some() {
                        tool.raw_input = raw_input;
                    }
                    if raw_output.is_some() {
                        tool.raw_output = raw_output;
                    }
                    if let Some(content) = content {
                        tool.content = content;
                    }
                }
            }
            AgentMessage::ResponseComplete => {
                self.flush_streaming();
                self.loading = false;
                self.status = "ready".into();
            }
            AgentMessage::SessionsList(sessions) => {
                self.sessions = sessions;
                self.sessions_selected = self
                    .sessions_selected
                    .min(self.sessions.len().saturating_sub(1));
                self.view = View::Sessions;
                self.status = "sessions".into();
            }
            AgentMessage::ExtensionsList(extensions) => {
                self.extensions = extensions;
                self.extensions_selected = self
                    .extensions_selected
                    .min(self.extensions.len().saturating_sub(1));
                self.view = View::Extensions;
                self.status = "extensions".into();
            }
            AgentMessage::Error(error) => {
                self.loading = false;
                self.status = "error".into();
                self.push_notice(NoticeKind::Error, "Provider error".into(), error);
                self.view = View::Chat;
            }
        }
    }

    fn start_session(&mut self) {
        self.loading = true;
        self.status = "starting session".into();
        let _ = self.cmd_tx.send(ClientCommand::CreateSession);
    }

    fn clear_chat(&mut self) {
        self.reset_session_state();
        self.push_message(Role::System, "Chat cleared.".into());
    }

    fn reset_session_state(&mut self) {
        self.timeline.clear();
        self.streaming.clear();
        self.selected_tool_call = None;
        self.expanded_tool_call = false;
        self.scrollback = 0;
        self.expanded_scroll = 0;
    }

    fn start_new_session(&mut self) {
        self.reset_session_state();
        self.has_session = false;
        self.start_session();
    }

    fn push_message(&mut self, role: Role, content: String) {
        self.timeline.push(TimelineItem::Message { role, content });
    }

    fn push_notice(&mut self, kind: NoticeKind, title: String, body: String) {
        self.flush_streaming();
        self.timeline
            .push(TimelineItem::Notice(Notice { title, body, kind }));
        self.scrollback = 0;
    }

    fn flush_streaming(&mut self) {
        if !self.streaming.is_empty() {
            let content = std::mem::take(&mut self.streaming);
            self.push_message(Role::Assistant, content);
        }
    }

    fn turn_count(&self) -> usize {
        self.timeline
            .iter()
            .filter(|item| matches!(item, TimelineItem::Message { .. } | TimelineItem::Notice(_)))
            .count()
    }

    fn tool_call_count(&self) -> usize {
        self.timeline
            .iter()
            .filter(|item| matches!(item, TimelineItem::ToolCall(_)))
            .count()
    }

    fn selected_tool(&self) -> Option<&ToolCall> {
        let selected = self.selected_tool_call?;
        self.timeline
            .iter()
            .filter_map(|item| match item {
                TimelineItem::ToolCall(tool) => Some(tool),
                _ => None,
            })
            .nth(selected)
    }

    fn move_tool_selection(&mut self, direction: isize) {
        let count = self.tool_call_count();
        if count == 0 {
            self.selected_tool_call = None;
            return;
        }
        let current = self
            .selected_tool_call
            .unwrap_or(if direction < 0 { count } else { 0 });
        let next = if direction < 0 {
            current.saturating_sub(1)
        } else {
            (current + 1).min(count - 1)
        };
        self.selected_tool_call = Some(next);
    }

    fn filtered_providers(&self) -> Vec<&ProviderInfo> {
        let query = self.provider_search.to_lowercase();
        self.providers
            .iter()
            .filter(|p| {
                query.is_empty()
                    || p.name.to_lowercase().contains(&query)
                    || p.id.to_lowercase().contains(&query)
            })
            .collect()
    }

    fn ensure_models(&mut self) {
        if self.models.is_empty() {
            let mut models: Vec<String> = self
                .providers
                .iter()
                .flat_map(|provider| provider.models.iter().cloned())
                .collect();
            models.sort();
            models.dedup();
            self.models = models;
        }
    }

    fn filtered_models(&self) -> Vec<&String> {
        let query = self.model_search.to_lowercase();
        self.models
            .iter()
            .filter(|model| query.is_empty() || model.to_lowercase().contains(&query))
            .collect()
    }

    fn handle_mouse(&mut self, event: MouseEvent) {
        if self.view != View::Chat || !self.mouse_in_main_content(event.column, event.row) {
            return;
        }

        let scroll = if self.expanded_tool_call {
            &mut self.expanded_scroll
        } else {
            &mut self.scrollback
        };

        match event.kind {
            MouseEventKind::ScrollUp => *scroll = scroll.saturating_add(3),
            MouseEventKind::ScrollDown => *scroll = scroll.saturating_sub(3),
            _ => {}
        }
    }

    fn mouse_in_main_content(&self, column: u16, row: u16) -> bool {
        let Ok((width, height)) = crossterm::terminal::size() else {
            return false;
        };
        let horizontal_padding = width.min(2);
        let vertical_padding = height.min(1);
        let content_x = horizontal_padding;
        let content_y = vertical_padding.saturating_add(2);
        let content_width = width.saturating_sub(horizontal_padding * 2);
        let content_height = height
            .saturating_sub(vertical_padding * 2)
            .saturating_sub(5);

        column >= content_x
            && column < content_x.saturating_add(content_width)
            && row >= content_y
            && row < content_y.saturating_add(content_height)
    }
}

pub async fn run_tui() -> Result<()> {
    let (cmd_tx, msg_rx) = spawn_acp_client(std::env::current_exe()?);
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    stdout().execute(EnableMouseCapture)?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;

    let mut app = App::new(cmd_tx.clone(), msg_rx);
    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(120));
    let _ = cmd_tx.send(ClientCommand::Initialize);

    loop {
        terminal.draw(|frame| render(frame, &app))?;
        if app.should_quit {
            break;
        }

        tokio::select! {
            _ = tick.tick() => app.tick = app.tick.wrapping_add(1),
            event = events.next() => {
                match event {
                    Some(Ok(CrosstermEvent::Key(key))) => app.handle_key(key.code, key.modifiers),
                    Some(Ok(CrosstermEvent::Mouse(mouse))) => app.handle_mouse(mouse),
                    _ => {}
                }
            }
            msg = app.msg_rx.recv() => {
                if let Some(msg) = msg {
                    app.handle_agent_message(msg);
                } else {
                    break;
                }
            }
        }
    }

    let _ = cmd_tx.send(ClientCommand::Shutdown);
    disable_raw_mode()?;
    stdout().execute(DisableMouseCapture)?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}
