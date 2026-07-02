use agent_client_protocol::schema::{ToolCallStatus, ToolKind};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{BorderType, Paragraph, Wrap};
use ratatui::Frame;

use super::markdown::push_markdown;
use super::style::*;
use super::views::{expanded_tool_lines, render_help_menu, render_slash_popover};
use super::{App, Notice, NoticeKind, Role, TimelineItem, ToolCall, View};

pub(super) fn render_chat(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .split(area);

    render_header(
        frame,
        chunks[0],
        &app.status,
        app.loading,
        app.tick,
        app.turn_count(),
    );
    match (app.expanded_tool_call, app.selected_tool()) {
        (true, Some(tool)) => render_tool_expanded(frame, chunks[1], tool, app.expanded_scroll),
        _ => render_messages(frame, chunks[1], app),
    }
    render_input(frame, chunks[2], app);
    render_slash_popover(frame, area, app);
    render_help_menu(frame, area, app);
}

fn render_header(
    frame: &mut Frame,
    area: Rect,
    status: &str,
    loading: bool,
    tick: usize,
    turns: usize,
) {
    let width = area.width as usize;
    let left_width = width.saturating_mul(7) / 10;
    let right_width = width.saturating_sub(left_width);
    let row = Layout::horizontal([
        Constraint::Length(left_width as u16),
        Constraint::Length(right_width as u16),
    ])
    .split(area);
    let status_color = match status {
        "ready" => TEAL,
        "error" => CRANBERRY,
        _ => TEXT_DIM,
    };
    let mut left = vec![
        Span::styled(
            "goose",
            Style::default()
                .fg(TEXT_PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" · ", fg(RULE_COLOR)),
        Span::styled(
            truncate(status, left_width.saturating_sub(10)),
            fg(status_color),
        ),
    ];
    if loading {
        left.push(Span::raw(" "));
        left.push(Span::styled(SPINNER[tick % SPINNER.len()], fg(TEAL)));
    }
    frame.render_widget(Paragraph::new(Line::from(left)), row[0]);
    let right = if turns > 1 {
        format!("{turns} turns  /help commands")
    } else {
        "/help commands".to_string()
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            truncate(&right, right_width),
            fg(TEXT_DIM),
        )))
        .alignment(Alignment::Right),
        row[1],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled("─".repeat(width), fg(RULE_COLOR)))),
        Rect {
            y: area.y + 1,
            height: 1,
            ..area
        },
    );
}

fn render_messages(frame: &mut Frame, area: Rect, app: &App) {
    let width = area.width as usize;
    let content_width = width.saturating_sub(4).max(10);
    let mut lines = Vec::new();
    let mut tool_index = 0;

    for (index, item) in app.timeline.iter().enumerate() {
        let previous_is_tool = index
            .checked_sub(1)
            .and_then(|previous| app.timeline.get(previous))
            .is_some_and(|item| matches!(item, TimelineItem::ToolCall(_)));
        let next_is_tool = app
            .timeline
            .get(index + 1)
            .is_some_and(|item| matches!(item, TimelineItem::ToolCall(_)));
        if !lines.is_empty() && !(previous_is_tool && matches!(item, TimelineItem::ToolCall(_))) {
            lines.push(Line::from(""));
        }
        match item {
            TimelineItem::Message { role, content } => match role {
                Role::User => push_user_message(&mut lines, content, content_width),
                Role::Assistant => push_markdown(&mut lines, content, content_width),
                Role::System => lines.push(Line::from(Span::styled(
                    truncate_flat(content, width),
                    italic(TEXT_DIM),
                ))),
            },
            TimelineItem::ToolCall(tool) => {
                push_tool_call(
                    &mut lines,
                    tool,
                    width,
                    app.selected_tool_call == Some(tool_index),
                    previous_is_tool,
                    next_is_tool,
                );
                tool_index += 1;
            }
            TimelineItem::Notice(notice) => push_notice(&mut lines, notice, width),
        }
    }

    if !app.streaming.is_empty() {
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        push_markdown(&mut lines, &app.streaming, content_width);
    }

    let max_scroll = lines.len().saturating_sub(area.height as usize);
    let scroll = max_scroll.saturating_sub(app.scrollback.min(max_scroll)) as u16;
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((scroll, 0))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn push_user_message(lines: &mut Vec<Line<'static>>, content: &str, width: usize) {
    lines.push(Line::from(vec![
        Span::styled("› ", bold(CRANBERRY)),
        Span::styled(truncate_flat(content, width), fg(TEXT_PRIMARY)),
    ]));
}

fn push_notice(lines: &mut Vec<Line<'static>>, notice: &Notice, width: usize) {
    let safe_width = width.max(10);
    let inner_width = safe_width.saturating_sub(4).max(6);
    let border_color = match notice.kind {
        NoticeKind::Info => TEAL,
        NoticeKind::Error => CRANBERRY,
    };
    let label = match notice.kind {
        NoticeKind::Info => "notice",
        NoticeKind::Error => "error",
    };
    let h_rule = "─".repeat(safe_width.saturating_sub(2));
    lines.push(Line::from(Span::styled(
        format!("╭{h_rule}╮"),
        fg(border_color),
    )));
    lines.push(Line::from(vec![
        Span::styled("│ ", fg(border_color)),
        Span::styled(label, bold(border_color)),
        Span::styled(" · ", fg(RULE_COLOR)),
        Span::styled(
            truncate_flat(&notice.title, inner_width.saturating_sub(label.len() + 3)),
            bold(TEXT_PRIMARY),
        ),
    ]));

    for wrapped in notice
        .body
        .lines()
        .flat_map(|line| wrap_words(line, inner_width.saturating_sub(2)))
    {
        lines.push(Line::from(vec![
            Span::styled("│ ", fg(border_color)),
            Span::styled(truncate_flat(&wrapped, inner_width), fg(TEXT_SECONDARY)),
        ]));
    }

    lines.push(Line::from(Span::styled(
        format!("╰{h_rule}╯"),
        fg(border_color),
    )));
}

fn push_tool_call(
    lines: &mut Vec<Line<'static>>,
    tool: &ToolCall,
    width: usize,
    selected: bool,
    previous_is_tool: bool,
    next_is_tool: bool,
) {
    let safe_width = width.max(10);
    let inner_width = safe_width.saturating_sub(4).max(6);
    let border_color = if selected {
        GOLD
    } else if matches!(tool.status, ToolCallStatus::Failed) {
        CRANBERRY
    } else {
        CEDAR
    };
    let connector_color = if matches!(tool.status, ToolCallStatus::Failed) {
        CRANBERRY
    } else {
        CEDAR
    };
    let h_rule = "─".repeat(safe_width.saturating_sub(2));
    if selected {
        lines.push(Line::from(Span::styled(
            format!("╭{h_rule}╮"),
            fg(border_color),
        )));
    }

    let connector = match (previous_is_tool, next_is_tool) {
        (true, true) => "│ ",
        (true, false) => "╰─",
        (false, true) => "╭─",
        (false, false) => "  ",
    };
    let (status, status_color) = tool_status(tool.status);
    let kind = tool_kind_label(tool.kind);
    let hint_text = if selected { "space to expand" } else { "" };
    let fixed_len = display_width(connector)
        + kind.chars().count()
        + status.chars().count()
        + hint_text.chars().count()
        + 6;
    let title = truncate_flat(&tool.title, inner_width.saturating_sub(fixed_len).max(4));
    let used = display_width(&format!("{connector}{kind} {title} {status}{hint_text}"));
    let spacer = " ".repeat(inner_width.saturating_sub(used));

    lines.push(Line::from(vec![
        Span::styled(connector, fg(connector_color)),
        Span::styled(kind, fg(TEXT_DIM)),
        Span::raw(" "),
        Span::styled(
            title,
            Style::default()
                .fg(TEXT_SECONDARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(status, fg(status_color)),
        Span::raw(spacer),
        Span::styled(hint_text, italic(GOLD)),
        Span::raw("  "),
    ]));
    if selected {
        lines.push(Line::from(Span::styled(
            format!("╰{h_rule}╯"),
            fg(border_color),
        )));
    }
}

fn tool_status(status: ToolCallStatus) -> (&'static str, Color) {
    match status {
        ToolCallStatus::InProgress => ("running", GOLD),
        ToolCallStatus::Completed => ("done", TEAL),
        ToolCallStatus::Failed => ("failed", CRANBERRY),
        _ => ("pending", TEXT_DIM),
    }
}

fn tool_kind_label(kind: ToolKind) -> &'static str {
    match kind {
        ToolKind::Read => "read",
        ToolKind::Edit => "edit",
        ToolKind::Delete => "delete",
        ToolKind::Move => "move",
        ToolKind::Search => "search",
        ToolKind::Execute => "run",
        ToolKind::Think => "think",
        ToolKind::Fetch => "fetch",
        ToolKind::SwitchMode => "mode",
        ToolKind::Other => "tool",
        _ => "tool",
    }
}

fn render_tool_expanded(frame: &mut Frame, area: Rect, tool: &ToolCall, scroll_offset: usize) {
    let width = area.width as usize;
    let height = area.height as usize;
    let content_width = width.saturating_sub(4).max(10);
    let body_height = height.saturating_sub(4).max(1);
    let mut body = expanded_tool_lines(tool, content_width);
    if body.is_empty() {
        body.push(Line::from(Span::styled(
            "(no details yet)",
            italic(TEXT_DIM),
        )));
    }

    let total = body.len();
    let content_height = if total > body_height {
        body_height.saturating_sub(2).max(1)
    } else {
        body_height
    };
    let end = total
        .saturating_sub(scroll_offset)
        .max(content_height)
        .min(total);
    let start = end.saturating_sub(content_height);

    let mut lines = vec![Line::from(vec![
        Span::styled("•", fg(tool_status(tool.status).1)),
        Span::styled(format!(" {:?}", tool.status), fg(TEXT_DIM)),
        Span::raw("  "),
        Span::styled(
            truncate_flat(&tool.title, content_width.saturating_sub(18)),
            Style::default()
                .fg(TEXT_PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
    ])];
    lines.push(Line::from(Span::styled(
        "─".repeat(content_width),
        fg(RULE_COLOR),
    )));

    if total > body_height {
        let above = start;
        lines.push(Line::from(Span::styled(
            if above > 0 {
                format!("▲ {above} more (↑)")
            } else {
                String::new()
            },
            fg(TEXT_DIM),
        )));
    }
    lines.extend(body[start..end].iter().cloned());
    for _ in 0..content_height.saturating_sub(end - start) {
        lines.push(Line::from(""));
    }
    if total > body_height {
        let below = total.saturating_sub(end);
        lines.push(Line::from(Span::styled(
            if below > 0 {
                format!("▼ {below} more (↓)")
            } else {
                String::new()
            },
            fg(TEXT_DIM),
        )));
    }

    let block = ui_block(GOLD, BorderType::Rounded, 1);
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_input(frame: &mut Frame, area: Rect, app: &App) {
    let block = ui_block(RULE_COLOR, BorderType::Rounded, 2);
    let input_area = block.inner(area);
    frame.render_widget(block, area);

    let text = if app.input.is_empty() {
        vec![
            Span::styled("› ", bold(CRANBERRY)),
            Span::styled("Type a message or /help for commands…", fg(TEXT_DIM)),
        ]
    } else {
        vec![
            Span::styled("› ", bold(CRANBERRY)),
            Span::styled(
                truncate(&app.input, input_area.width.saturating_sub(2) as usize),
                fg(TEXT_PRIMARY),
            ),
        ]
    };
    frame.render_widget(Paragraph::new(Line::from(text)), input_area);
    if app.view == View::Chat && !app.loading {
        let x = input_area.x + 2 + (app.cursor as u16).min(input_area.width.saturating_sub(3));
        frame.set_cursor_position((x, input_area.y));
    }
}
