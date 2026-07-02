use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, BorderType, Borders, Padding};

pub(super) const BACKGROUND: Color = Color::Rgb(0, 0, 0);
pub(super) const CRANBERRY: Color = Color::Rgb(238, 238, 238);
pub(super) const TEAL: Color = Color::Rgb(245, 245, 245);
pub(super) const GOLD: Color = Color::Rgb(210, 210, 210);
pub(super) const TEXT_PRIMARY: Color = Color::White;
pub(super) const TEXT_SECONDARY: Color = Color::Rgb(188, 188, 188);
pub(super) const TEXT_DIM: Color = Color::Rgb(112, 112, 112);
pub(super) const RULE_COLOR: Color = Color::Rgb(38, 38, 38);
pub(super) const CEDAR: Color = Color::Rgb(72, 72, 72);
pub(super) const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub(super) fn fg(color: Color) -> Style {
    Style::default().fg(color)
}
pub(super) fn bold(color: Color) -> Style {
    fg(color).add_modifier(Modifier::BOLD)
}
pub(super) fn italic(color: Color) -> Style {
    fg(color).add_modifier(Modifier::ITALIC)
}

pub(super) fn ui_block(border: Color, border_type: BorderType, padding: u16) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(border_type)
        .border_style(fg(border))
        .padding(Padding::horizontal(padding))
}

pub(super) fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if line.is_empty() {
            line.push_str(word);
        } else if line.len() + word.len() < width.max(1) {
            line.push(' ');
            line.push_str(word);
        } else {
            lines.push(std::mem::take(&mut line));
            line.push_str(word);
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

pub(super) fn display_width(text: &str) -> usize {
    text.chars().count()
}

pub(super) fn truncate(text: &str, max: usize) -> String {
    match text.chars().count() {
        count if count <= max => text.to_string(),
        _ if max > 1 => format!("{}…", text.chars().take(max - 1).collect::<String>()),
        _ => "…".into(),
    }
}

pub(super) fn truncate_flat(text: &str, max: usize) -> String {
    truncate(&text.split_whitespace().collect::<Vec<_>>().join(" "), max)
}
pub(super) fn provider_columns(width: u16) -> usize {
    ((width.saturating_sub(4) / 38).max(1)) as usize
}
pub(super) fn terminal_width() -> u16 {
    crossterm::terminal::size().map(|(w, _)| w).unwrap_or(80)
}

pub(super) fn padded(area: Rect) -> Rect {
    let horizontal = area.width.min(2);
    let vertical = area.height.min(1);
    Rect {
        x: area.x + horizontal,
        y: area.y + vertical,
        width: area.width.saturating_sub(horizontal * 2),
        height: area.height.saturating_sub(vertical * 2),
    }
}

pub(super) fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}
