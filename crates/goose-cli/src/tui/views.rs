use agent_client_protocol::schema::{
    ContentBlock, EmbeddedResourceResource, ToolCallContent as AcpToolCallContent,
};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use super::acp::ProviderInfo;
use super::chat::render_chat;
use super::slash::{SlashCommand, SLASH_COMMANDS};
use super::style::*;
use super::{App, ToolCall, View};

pub(super) fn render(frame: &mut Frame, app: &App) {
    let full = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(BACKGROUND).fg(TEXT_PRIMARY)),
        full,
    );
    let area = padded(full);
    match app.view {
        View::Splash => render_splash(frame, full, app),
        View::Chat => render_chat(frame, area, app),
        View::Providers => render_providers(frame, area, app),
        View::Models => render_picker(
            frame,
            area,
            "Models",
            "Choose a model for your provider",
            "search…",
            "type to filter · ↑↓ navigate · enter select · esc back",
            &app.model_search,
            app.models_selected,
            app.filtered_models()
                .iter()
                .map(|m| (m.as_str(), "", false))
                .collect(),
            "No matches",
        ),
        View::Sessions => render_picker(
            frame,
            area,
            "Sessions",
            "recent sessions",
            "",
            "↑↓ navigate · enter resume · n new · esc back",
            "",
            app.sessions_selected,
            app.sessions
                .iter()
                .map(|s| (s.title.as_str(), s.updated_at.as_str(), false))
                .collect(),
            "Nothing here yet",
        ),
        View::Extensions => render_picker(
            frame,
            area,
            "Extensions",
            "session extensions",
            "",
            "↑↓ navigate · space toggle · esc back",
            "",
            app.extensions_selected,
            app.extensions
                .iter()
                .map(|e| (e.name.as_str(), e.ext_type.as_str(), e.enabled))
                .collect(),
            "Nothing here yet",
        ),
    }
}

fn heading(title: &str, subtitle: &str) -> Paragraph<'static> {
    Paragraph::new(vec![
        Line::from(Span::styled(title.to_string(), bold(TEXT_PRIMARY))),
        Line::from(Span::styled(subtitle.to_string(), fg(TEXT_DIM))),
    ])
    .alignment(Alignment::Center)
}

fn selected_style(selected: bool) -> Style {
    fg(if selected {
        TEXT_PRIMARY
    } else {
        TEXT_SECONDARY
    })
    .add_modifier(if selected {
        Modifier::BOLD
    } else {
        Modifier::empty()
    })
}

fn render_search(frame: &mut Frame, area: Rect, search: &str, placeholder: &str) {
    let area = centered(area, area.width.saturating_sub(4).min(60), 3);
    let block = ui_block(RULE_COLOR, BorderType::Rounded, 2);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("› ", bold(CRANBERRY)),
            Span::styled(
                if search.is_empty() {
                    placeholder
                } else {
                    search
                },
                fg(if search.is_empty() {
                    TEXT_DIM
                } else {
                    TEXT_PRIMARY
                }),
            ),
        ])),
        inner,
    );
}

fn render_picker(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    subtitle: &str,
    placeholder: &str,
    help: &str,
    search: &str,
    selected: usize,
    items: Vec<(&str, &str, bool)>,
    empty: &str,
) {
    let constraints = if placeholder.is_empty() {
        vec![
            Constraint::Length(4),
            Constraint::Min(1),
            Constraint::Length(2),
        ]
    } else {
        vec![
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(2),
        ]
    };
    let chunks = Layout::vertical(constraints).split(area);
    frame.render_widget(heading(title, subtitle), chunks[0]);
    let (list_chunk, help_chunk) = if placeholder.is_empty() {
        (chunks[1], chunks[2])
    } else {
        render_search(frame, chunks[1], search, placeholder);
        (chunks[2], chunks[3])
    };
    let list_width = area.width.saturating_sub(4).min(86);
    let list = centered(list_chunk, list_width, list_chunk.height);
    let visible = list.height.saturating_sub(2) as usize;
    let scroll = selected.saturating_sub(visible.saturating_sub(1));
    let lines = if items.is_empty() {
        vec![Line::from(Span::styled(empty.to_string(), fg(TEXT_DIM)))]
    } else {
        items
            .iter()
            .enumerate()
            .skip(scroll)
            .take(visible)
            .map(|(idx, (name, meta, enabled))| {
                let name = if name.is_empty() {
                    "Untitled Session"
                } else {
                    name
                };
                Line::from(vec![
                    Span::styled(if idx == selected { "› " } else { "  " }, bold(CRANBERRY)),
                    Span::styled(if *enabled { "✓ " } else { "" }, fg(TEAL)),
                    Span::styled(
                        truncate_flat(
                            name,
                            if placeholder.is_empty() {
                                list_width as usize / 2
                            } else {
                                list_width.saturating_sub(8) as usize
                            },
                        ),
                        selected_style(idx == selected),
                    ),
                    Span::styled(format!("  {meta}"), fg(TEXT_DIM)),
                ])
            })
            .collect()
    };
    frame.render_widget(
        Paragraph::new(lines).block(ui_block(RULE_COLOR, BorderType::Rounded, 1)),
        list,
    );
    frame.render_widget(
        Paragraph::new(help.to_string())
            .style(fg(TEXT_DIM))
            .alignment(Alignment::Center),
        help_chunk,
    );
}

pub(super) fn render_providers(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(4),
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .split(area);
    frame.render_widget(
        heading("goose", "Connect an AI model provider to get started"),
        chunks[0],
    );
    render_search(frame, chunks[1], &app.provider_search, "search providers…");
    render_provider_grid(frame, chunks[2], app);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "↑↓←→ navigate · enter select · type to search · esc clear/back",
            fg(TEXT_DIM),
        )))
        .alignment(Alignment::Center),
        chunks[3],
    );
}

fn render_provider_grid(frame: &mut Frame, area: Rect, app: &App) {
    let providers = app.filtered_providers();
    if providers.is_empty() {
        frame.render_widget(
            Paragraph::new("No matching providers found")
                .style(fg(TEXT_DIM))
                .alignment(Alignment::Center),
            area,
        );
        return;
    }
    let (card_width, card_height) = (36u16, 8u16);
    let columns = provider_columns(area.width);
    let rows_visible = ((area.height as usize + 1) / (card_height as usize + 1)).max(1);
    let total_rows = providers.len().div_ceil(columns);
    let scroll_row =
        (app.providers_selected / columns).saturating_sub(rows_visible.saturating_sub(1));
    let visible_rows = rows_visible.min(total_rows.saturating_sub(scroll_row));
    let grid = centered(
        area,
        ((columns as u16 * card_width) + (columns.saturating_sub(1) as u16 * 2)).min(area.width),
        (visible_rows as u16 * (card_height + 1))
            .saturating_sub(1)
            .min(area.height),
    );
    for row in 0..visible_rows {
        for col in 0..columns {
            let idx = (scroll_row + row) * columns + col;
            let Some(provider) = providers.get(idx) else {
                continue;
            };
            let rect = Rect {
                x: grid.x + col as u16 * (card_width + 2),
                y: grid.y + row as u16 * (card_height + 1),
                width: card_width,
                height: card_height,
            };
            if rect.x + card_width <= area.x + area.width
                && rect.y + card_height <= area.y + area.height
            {
                render_provider_card(frame, rect, provider, idx == app.providers_selected);
            }
        }
    }
    if scroll_row > 0 {
        scroll_hint(
            frame,
            area,
            area.y,
            format!("▲ {} more above", scroll_row * columns),
        );
    }
    if scroll_row + visible_rows < total_rows {
        scroll_hint(
            frame,
            area,
            area.y + area.height.saturating_sub(1),
            format!(
                "▼ {} more below",
                providers
                    .len()
                    .saturating_sub((scroll_row + visible_rows) * columns)
            ),
        );
    }
}

fn scroll_hint(frame: &mut Frame, area: Rect, y: u16, text: String) {
    frame.render_widget(
        Paragraph::new(text)
            .style(fg(TEXT_DIM))
            .alignment(Alignment::Center),
        Rect {
            y,
            height: 1,
            ..area
        },
    );
}

fn render_provider_card(frame: &mut Frame, area: Rect, provider: &ProviderInfo, selected: bool) {
    let block = ui_block(
        if selected { GOLD } else { RULE_COLOR },
        BorderType::Plain,
        1,
    );
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                truncate(&provider.name, inner.width.saturating_sub(2) as usize),
                selected_style(selected),
            ),
            Span::styled(if provider.configured { "✓" } else { "" }, fg(TEAL)),
        ])),
        Rect { height: 1, ..inner },
    );
    frame.render_widget(
        Paragraph::new(truncate(&provider.id, inner.width as usize)).style(fg(TEXT_DIM)),
        Rect {
            y: inner.y + 2,
            height: 1,
            ..inner
        },
    );
    frame.render_widget(
        Paragraph::new(truncate_flat(
            &provider.description,
            inner.width as usize * 3,
        ))
        .style(fg(TEXT_DIM))
        .wrap(Wrap { trim: true }),
        Rect {
            y: inner.y + 4,
            height: 3,
            ..inner
        },
    );
}

pub(super) fn render_help_menu(frame: &mut Frame, area: Rect, app: &App) {
    if app.view == View::Chat && app.show_help_menu {
        render_command_menu(
            frame,
            centered(area, 56.min(area.width), 12.min(area.height)),
            SLASH_COMMANDS,
            None,
        );
    }
}

pub(super) fn render_slash_popover(frame: &mut Frame, area: Rect, app: &App) {
    if app.view != View::Chat {
        return;
    }
    let commands = app.slash_commands();
    if commands.is_empty() {
        return;
    }
    let height = (commands.len() as u16 + 2).min(10);
    let visible = height.saturating_sub(2) as usize;
    let selected = app.slash_selected.min(commands.len() - 1);
    let start = selected.saturating_sub(visible.saturating_sub(1));
    render_command_menu(
        frame,
        Rect {
            x: area.x + 1,
            y: area.y + area.height.saturating_sub(height + 3),
            width: 52.min(area.width.saturating_sub(2)).max(24),
            height,
        },
        &commands[start..(start + visible).min(commands.len())],
        Some(selected - start),
    );
}

fn render_command_menu(
    frame: &mut Frame,
    area: Rect,
    commands: &[SlashCommand],
    selected: Option<usize>,
) {
    let inner_width = area.width.saturating_sub(4) as usize;
    let lines = commands
        .iter()
        .enumerate()
        .map(|(index, command)| {
            let is_selected = selected == Some(index);
            let style = Style::default()
                .fg(if is_selected { BACKGROUND } else { GOLD })
                .bg(if is_selected { GOLD } else { BACKGROUND })
                .add_modifier(Modifier::BOLD);
            let description_style = Style::default()
                .fg(if is_selected { BACKGROUND } else { TEXT_DIM })
                .bg(if is_selected { GOLD } else { BACKGROUND });
            Line::from(vec![
                Span::styled(if is_selected { "› " } else { "  " }, style),
                Span::styled(command.name, style),
                Span::styled("  ", description_style),
                Span::styled(
                    truncate(
                        command.description,
                        inner_width.saturating_sub(command.name.len() + 4),
                    ),
                    description_style,
                ),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(fg(GOLD))
                .title(Span::styled(" commands ", fg(TEXT_SECONDARY))),
        ),
        area,
    );
}

const GOOSE_FRAMES: &[&[&str]] = &[
    &[
        r#"             __"#,
        r#"          __/o )_"#,
        r#"   .-.___/  ___/ \"#,
        r#"  /  _     /      \"#,
        r#" /__/ \___/  _/\_  \"#,
        r#"      `---' /    \__)"#,
        r#"            `-._,"#,
    ],
    &[
        r#"          __"#,
        r#"       __/o )_"#,
        r#"  _.-_/  ___/ \"#,
        r#" /  _     /    _\"#,
        r#"/__/ \___/  _/  `"#,
        r#"     `---' /"#,
        r#"           `-._,"#,
    ],
    &[
        r#"              __"#,
        r#"           __/o )_"#,
        r#"    __..__/  ___/ \"#,
        r#" __/  _      /     \"#,
        r#"    _/ \___/  _/\_ \"#,
        r#"   /    `---' /    \_)"#,
        r#"        _.-'"#,
    ],
    &[
        r#"           __"#,
        r#"        __/o )_"#,
        r#" .-.___/  ___/ \"#,
        r#"/  _      /      \"#,
        r#"  / \___/  _/\_  \"#,
        r#" /   `---' /    \__)"#,
        r#"       `-._,"#,
    ],
];

pub(super) fn render_splash(frame: &mut Frame, area: Rect, app: &App) {
    let tick = app.tick;
    let prompt_height = 3.min(area.height);
    let status_height = 3.min(area.height.saturating_sub(prompt_height));
    let bottom_margin = area
        .height
        .saturating_sub(prompt_height + status_height + 8)
        .min(3);
    let lower_content_height = prompt_height + status_height + bottom_margin;
    let flight_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: area.height.saturating_sub(lower_content_height),
    };

    render_flying_goose(frame, flight_area, tick);
    render_splash_status(
        frame,
        area,
        status_height,
        prompt_height + bottom_margin,
        tick,
    );
    render_splash_prompt(
        frame,
        area,
        prompt_height,
        bottom_margin,
        &app.input,
        app.cursor,
    );
}

fn render_flying_goose(frame: &mut Frame, area: Rect, tick: usize) {
    if area.is_empty() {
        return;
    }

    let goose = GOOSE_FRAMES[(tick / 2) % GOOSE_FRAMES.len()];
    let goose_width = goose
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    let goose_height = goose.len() as u16;
    let position =
        ((tick * 2) % (area.width as usize + goose_width + 8)) as isize - goose_width as isize;
    let y = area
        .y
        .saturating_add(area.height.saturating_sub(goose_height) / 2)
        .saturating_sub(usize::from(matches!(tick % 12, 3..=5 | 9..=11)) as u16);

    if y >= area.y.saturating_add(area.height) {
        return;
    }
    let x = position.max(0) as u16;
    if x >= area.width {
        return;
    }
    let visible_height = area
        .y
        .saturating_add(area.height)
        .saturating_sub(y)
        .min(goose_height);
    let visible_width = area.width.saturating_sub(x) as usize;
    let lines = goose
        .iter()
        .take(visible_height as usize)
        .map(|line| {
            Line::from(Span::styled(
                line.chars()
                    .skip(if position < 0 {
                        position.saturating_abs() as usize
                    } else {
                        0
                    })
                    .take(visible_width)
                    .collect::<String>(),
                fg(TEXT_PRIMARY).bg(BACKGROUND),
            ))
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines),
        Rect {
            x: area.x + x,
            y,
            width: visible_width as u16,
            height: visible_height,
        },
    );
}

fn render_splash_status(
    frame: &mut Frame,
    area: Rect,
    height: u16,
    bottom_offset: u16,
    tick: usize,
) {
    if height == 0 {
        return;
    }

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "·".repeat((area.width.saturating_sub(8) as usize).min(48)),
                fg(RULE_COLOR).bg(BACKGROUND),
            )),
            Line::from(vec![
                Span::styled("goose", bold(TEXT_PRIMARY).bg(BACKGROUND)),
                Span::styled(" / ", fg(RULE_COLOR).bg(BACKGROUND)),
                Span::styled("initializing", fg(TEXT_DIM).bg(BACKGROUND)),
                Span::raw(" "),
                Span::styled(
                    SPINNER[tick % SPINNER.len()],
                    fg(TEXT_SECONDARY).bg(BACKGROUND),
                ),
            ]),
        ])
        .alignment(Alignment::Center),
        Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(height + bottom_offset),
            width: area.width,
            height,
        },
    );
}

fn render_splash_prompt(
    frame: &mut Frame,
    area: Rect,
    height: u16,
    bottom_margin: u16,
    input: &str,
    cursor: usize,
) {
    if height == 0 {
        return;
    }

    let prompt = centered(
        Rect {
            height,
            y: area
                .y
                .saturating_add(area.height.saturating_sub(height + bottom_margin)),
            ..area
        },
        area.width.saturating_sub(12).max(32),
        height,
    );
    let block = ui_block(RULE_COLOR, BorderType::Rounded, 2);
    let input_area = block.inner(prompt);
    frame.render_widget(block, prompt);

    let text = if input.is_empty() {
        vec![
            Span::styled("› ", bold(CRANBERRY)),
            Span::styled("Start typing while goose wakes up…", fg(TEXT_DIM)),
        ]
    } else {
        vec![
            Span::styled("› ", bold(CRANBERRY)),
            Span::styled(
                truncate(input, input_area.width.saturating_sub(2) as usize),
                fg(TEXT_PRIMARY),
            ),
        ]
    };
    frame.render_widget(Paragraph::new(Line::from(text)), input_area);

    let x = input_area.x + 2 + (cursor as u16).min(input_area.width.saturating_sub(3));
    frame.set_cursor_position((x, input_area.y));
}

pub(super) fn expanded_tool_lines(tool: &ToolCall, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    push_expanded_section(
        &mut lines,
        "arguments",
        format_json(tool.raw_input.as_ref()),
        width,
    );
    push_expanded_section(&mut lines, "result", tool_result_text(tool), width);
    lines
}

fn push_expanded_section(lines: &mut Vec<Line<'static>>, label: &str, text: String, width: usize) {
    if !lines.is_empty() {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        label.to_string(),
        bold(TEXT_SECONDARY),
    )));
    if text.is_empty() {
        lines.push(Line::from(Span::styled("(empty)", italic(TEXT_DIM))));
    }
    for raw in text.lines() {
        for wrapped in wrap_words(raw, width) {
            lines.push(Line::from(Span::styled(wrapped, fg(TEXT_PRIMARY))));
        }
    }
}

fn format_json(value: Option<&serde_json::Value>) -> String {
    value
        .and_then(|value| serde_json::to_string_pretty(value).ok())
        .unwrap_or_default()
}

fn tool_result_text(tool: &ToolCall) -> String {
    let raw = format_json(tool.raw_output.as_ref());
    if !raw.is_empty() {
        return raw;
    }
    tool.content
        .iter()
        .filter_map(|item| match item {
            AcpToolCallContent::Content(content) => match &content.content {
                ContentBlock::Text(text) => Some(text.text.clone()),
                ContentBlock::ResourceLink(link) => Some(format!("link {}", link.uri)),
                ContentBlock::Image(image) => Some(format!(
                    "image ({}){}",
                    image.mime_type,
                    image
                        .uri
                        .as_deref()
                        .map(|uri| format!(" {uri}"))
                        .unwrap_or_default()
                )),
                ContentBlock::Audio(audio) => Some(format!("audio ({})", audio.mime_type)),
                ContentBlock::Resource(resource) => match &resource.resource {
                    EmbeddedResourceResource::TextResourceContents(text) => Some(text.text.clone()),
                    EmbeddedResourceResource::BlobResourceContents(blob) => {
                        Some(format!("blob {}", blob.uri))
                    }
                    _ => None,
                },
                _ => None,
            },
            AcpToolCallContent::Diff(diff) => {
                let mut lines = vec![format!("diff {}", diff.path.display())];
                if let Some(old) = &diff.old_text {
                    lines.extend(old.lines().map(|line| format!("- {line}")));
                }
                lines.extend(diff.new_text.lines().map(|line| format!("+ {line}")));
                Some(lines.join("\n"))
            }
            AcpToolCallContent::Terminal(terminal) => {
                Some(format!("▶ terminal: {}", terminal.terminal_id.0))
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}
