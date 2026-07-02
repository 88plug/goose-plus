use pulldown_cmark::{
    Alignment as MarkdownAlignment, CodeBlockKind, Event as MarkdownEvent,
    HeadingLevel as MarkdownHeadingLevel, Options, Parser, Tag, TagEnd,
};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use super::style::{
    bold, display_width, fg, italic, truncate, truncate_flat, wrap_words, GOLD, RULE_COLOR, TEAL,
    TEXT_DIM, TEXT_PRIMARY, TEXT_SECONDARY,
};

pub(super) fn push_markdown(lines: &mut Vec<Line<'static>>, text: &str, width: usize) {
    let mut renderer = MarkdownRenderer::new(width);
    renderer.render(text);
    lines.extend(renderer.lines);
}

struct MarkdownRenderer {
    lines: Vec<Line<'static>>,
    width: usize,
    spans: Vec<Span<'static>>,
    styles: Vec<Style>,
    list_stack: Vec<ListState>,
    line_prefix: Option<String>,
    continuation_prefix: String,
    quote_depth: usize,
    block: MarkdownBlock,
    link_urls: Vec<String>,
    code_lang: Option<String>,
    code_text: String,
    table: Option<TableState>,
}

struct ListState {
    next: Option<u64>,
}

#[derive(Default)]
enum MarkdownBlock {
    #[default]
    None,
    Paragraph,
    Heading(MarkdownHeadingLevel),
    Item,
    TableCell,
}

#[derive(Default)]
struct TableState {
    alignments: Vec<MarkdownAlignment>,
    rows: Vec<Vec<Vec<Span<'static>>>>,
    current_row: Vec<Vec<Span<'static>>>,
    current_cell: Vec<Span<'static>>,
}

impl MarkdownRenderer {
    fn new(width: usize) -> Self {
        Self {
            lines: Vec::new(),
            width: width.max(10),
            spans: Vec::new(),
            styles: vec![fg(TEXT_PRIMARY)],
            list_stack: Vec::new(),
            line_prefix: None,
            continuation_prefix: String::new(),
            quote_depth: 0,
            block: MarkdownBlock::None,
            link_urls: Vec::new(),
            code_lang: None,
            code_text: String::new(),
            table: None,
        }
    }

    fn render(&mut self, text: &str) {
        let options = Options::ENABLE_TABLES
            | Options::ENABLE_FOOTNOTES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_SMART_PUNCTUATION
            | Options::ENABLE_HEADING_ATTRIBUTES
            | Options::ENABLE_DEFINITION_LIST
            | Options::ENABLE_GFM;

        for event in Parser::new_ext(text, options) {
            self.handle_event(event);
        }
        self.finish_inline_block();
        self.finish_code_block();
        self.finish_table();
    }

    fn handle_event(&mut self, event: MarkdownEvent<'_>) {
        match event {
            MarkdownEvent::Start(tag) => self.start_tag(tag),
            MarkdownEvent::End(tag) => self.end_tag(tag),
            MarkdownEvent::Text(text) => self.push_text(text.as_ref()),
            MarkdownEvent::Code(code) => self.push_styled(code.as_ref(), bold(GOLD)),
            MarkdownEvent::InlineMath(math) => self.push_styled(&format!("${math}$"), italic(GOLD)),
            MarkdownEvent::DisplayMath(math) => {
                self.finish_inline_block();
                self.lines
                    .push(Line::from(Span::styled(format!("  {math}"), italic(GOLD))));
            }
            MarkdownEvent::Html(html) | MarkdownEvent::InlineHtml(html) => {
                self.push_styled(html.as_ref(), fg(TEXT_DIM));
            }
            MarkdownEvent::FootnoteReference(reference) => {
                self.push_styled(&format!("[{reference}]"), bold(TEAL));
            }
            MarkdownEvent::SoftBreak => self.push_text(" "),
            MarkdownEvent::HardBreak => self.push_text("\n"),
            MarkdownEvent::Rule => {
                self.finish_inline_block();
                self.lines.push(Line::from(Span::styled(
                    "─".repeat(self.width),
                    fg(RULE_COLOR),
                )));
            }
            MarkdownEvent::TaskListMarker(checked) => {
                self.push_styled(if checked { "☑ " } else { "☐ " }, fg(TEAL));
            }
        }
    }

    fn start_tag(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => self.block = MarkdownBlock::Paragraph,
            Tag::Heading { level, .. } => self.block = MarkdownBlock::Heading(level),
            Tag::BlockQuote(_) => self.quote_depth += 1,
            Tag::CodeBlock(kind) => {
                self.finish_inline_block();
                self.block = MarkdownBlock::None;
                self.code_lang = match kind {
                    CodeBlockKind::Fenced(lang) => {
                        let lang = lang.to_string();
                        Some(if lang.is_empty() { "code".into() } else { lang })
                    }
                    CodeBlockKind::Indented => Some("code".into()),
                };
                self.code_text.clear();
            }
            Tag::List(start) => self.list_stack.push(ListState { next: start }),
            Tag::Item => {
                self.block = MarkdownBlock::Item;
                self.finish_inline_block();
                let marker = self.next_list_prefix();
                let indent = "  ".repeat(self.list_stack.len().saturating_sub(1));
                self.line_prefix = Some(format!("{indent}{marker}"));
                self.continuation_prefix = format!(
                    "{}{}",
                    quote_prefix(self.quote_depth),
                    " ".repeat(indent.chars().count() + marker.chars().count())
                );
            }
            Tag::Emphasis => self.push_style(
                Style::default()
                    .fg(TEXT_SECONDARY)
                    .add_modifier(Modifier::ITALIC),
            ),
            Tag::Strong => self.push_style(
                Style::default()
                    .fg(TEXT_PRIMARY)
                    .add_modifier(Modifier::BOLD),
            ),
            Tag::Strikethrough => self.push_style(
                Style::default()
                    .fg(TEXT_DIM)
                    .add_modifier(Modifier::CROSSED_OUT),
            ),
            Tag::Superscript => self.push_style(fg(GOLD)),
            Tag::Subscript => self.push_style(fg(TEXT_DIM)),
            Tag::Link { dest_url, .. } => {
                self.link_urls.push(dest_url.to_string());
                self.push_style(fg(TEAL).add_modifier(Modifier::UNDERLINED));
            }
            Tag::Image { dest_url, .. } => {
                self.link_urls.push(dest_url.to_string());
                self.push_styled("[image: ", fg(TEXT_DIM));
                self.push_style(fg(TEAL).add_modifier(Modifier::UNDERLINED));
            }
            Tag::Table(alignments) => {
                self.finish_inline_block();
                self.table = Some(TableState {
                    alignments,
                    ..TableState::default()
                });
            }
            Tag::TableHead | Tag::TableRow => {
                if let Some(table) = &mut self.table {
                    table.current_row.clear();
                }
            }
            Tag::TableCell => {
                self.block = MarkdownBlock::TableCell;
                self.spans.clear();
            }
            Tag::HtmlBlock
            | Tag::FootnoteDefinition(_)
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::MetadataBlock(_) => {}
        }
    }

    fn end_tag(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.finish_inline_block(),
            TagEnd::Item => {
                self.finish_inline_block();
                self.continuation_prefix.clear();
            }
            TagEnd::Heading(_) => self.finish_heading(),
            TagEnd::BlockQuote(_) => self.quote_depth = self.quote_depth.saturating_sub(1),
            TagEnd::CodeBlock => self.finish_code_block(),
            TagEnd::List(_) => {
                self.finish_inline_block();
                self.list_stack.pop();
            }
            TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Superscript
            | TagEnd::Subscript => self.pop_style(),
            TagEnd::Link => self.finish_link(" (", ")"),
            TagEnd::Image => self.finish_link("](", ")"),
            TagEnd::TableCell => {
                if let Some(table) = &mut self.table {
                    table.current_cell = std::mem::take(&mut self.spans);
                    table
                        .current_row
                        .push(std::mem::take(&mut table.current_cell));
                }
                self.block = MarkdownBlock::None;
            }
            TagEnd::TableHead | TagEnd::TableRow => {
                if let Some(table) = &mut self.table {
                    table.rows.push(std::mem::take(&mut table.current_row));
                }
            }
            TagEnd::Table => self.finish_table(),
            TagEnd::HtmlBlock
            | TagEnd::FootnoteDefinition
            | TagEnd::DefinitionList
            | TagEnd::DefinitionListTitle
            | TagEnd::DefinitionListDefinition
            | TagEnd::MetadataBlock(_) => {}
        }
    }

    fn push_text(&mut self, text: &str) {
        if self.code_lang.is_some() {
            self.code_text.push_str(text);
        } else {
            self.push_styled(text, *self.styles.last().unwrap_or(&fg(TEXT_PRIMARY)));
        }
    }

    fn push_style(&mut self, style: Style) {
        self.styles.push(style);
    }

    fn pop_style(&mut self) {
        self.styles.pop();
    }

    fn finish_link(&mut self, before: &str, after: &str) {
        self.pop_style();
        if let Some(url) = self.link_urls.pop() {
            self.push_styled(&format!("{before}{url}{after}"), fg(TEXT_DIM));
        }
    }

    fn push_styled(&mut self, text: &str, style: Style) {
        self.spans.push(Span::styled(text.to_string(), style));
    }

    fn next_list_prefix(&mut self) -> String {
        match self
            .list_stack
            .last_mut()
            .and_then(|list| list.next.as_mut())
        {
            Some(next) => {
                let prefix = format!("{next}. ");
                *next += 1;
                prefix
            }
            None => "• ".to_string(),
        }
    }

    fn finish_inline_block(&mut self) {
        if self.spans.is_empty() {
            self.block = MarkdownBlock::None;
            return;
        }
        let quote_prefix = quote_prefix(self.quote_depth);
        let active_list_item = !self.continuation_prefix.is_empty();
        let first_prefix = self
            .line_prefix
            .take()
            .map(|prefix| format!("{quote_prefix}{prefix}"))
            .unwrap_or_else(|| {
                if active_list_item {
                    self.continuation_prefix.clone()
                } else {
                    quote_prefix.clone()
                }
            });
        let continuation_prefix = if active_list_item {
            self.continuation_prefix.clone()
        } else {
            quote_prefix
        };
        let prefix_width = display_width(&first_prefix).max(display_width(&continuation_prefix));
        let available = self.width.saturating_sub(prefix_width).max(1);
        let wrapped = wrap_spans(std::mem::take(&mut self.spans), available);
        for (index, line) in wrapped.into_iter().enumerate() {
            let prefix = if index == 0 {
                &first_prefix
            } else {
                &continuation_prefix
            };
            let mut spans = Vec::new();
            if !prefix.is_empty() {
                spans.push(Span::styled(prefix.clone(), fg(RULE_COLOR)));
            }
            spans.extend(line);
            self.lines.push(Line::from(spans));
        }
        self.block = MarkdownBlock::None;
    }

    fn finish_heading(&mut self) {
        let level = match self.block {
            MarkdownBlock::Heading(level) => level,
            _ => MarkdownHeadingLevel::H3,
        };
        let text = spans_plain_text(&self.spans);
        self.spans.clear();
        if text.is_empty() {
            self.block = MarkdownBlock::None;
            return;
        }

        let style = match level {
            MarkdownHeadingLevel::H1
            | MarkdownHeadingLevel::H2
            | MarkdownHeadingLevel::H3
            | MarkdownHeadingLevel::H4 => bold(TEXT_PRIMARY),
            MarkdownHeadingLevel::H5 => bold(TEXT_SECONDARY),
            MarkdownHeadingLevel::H6 => bold(TEXT_DIM),
        };

        self.push_heading_margin(1);
        for line in heading_lines(&text, level, self.width) {
            self.lines.push(Line::from(Span::styled(line, style)));
        }
        if let Some(rule) = heading_rule(level) {
            self.lines
                .push(Line::from(Span::styled(rule, fg(RULE_COLOR))));
        }
        self.block = MarkdownBlock::None;
    }

    fn push_heading_margin(&mut self, count: usize) {
        if self.lines.is_empty() {
            return;
        }
        for _ in 0..count {
            self.lines.push(Line::from(""));
        }
    }

    fn finish_code_block(&mut self) {
        if self.code_text.is_empty() && self.code_lang.is_none() {
            return;
        }
        let label = self.code_lang.take().unwrap_or_else(|| "code".into());
        self.lines.push(Line::from(vec![
            Span::styled("╭─ ", fg(RULE_COLOR)),
            Span::styled(label, italic(TEXT_DIM)),
        ]));
        for raw in self.code_text.trim_end_matches('\n').lines() {
            self.lines.push(Line::from(vec![
                Span::styled("│ ", fg(RULE_COLOR)),
                Span::styled(
                    truncate(raw, self.width.saturating_sub(2)),
                    fg(TEXT_SECONDARY),
                ),
            ]));
        }
        self.lines
            .push(Line::from(Span::styled("╰", fg(RULE_COLOR))));
        self.code_text.clear();
        self.block = MarkdownBlock::None;
    }

    fn finish_table(&mut self) {
        let Some(table) = self.table.take() else {
            return;
        };
        if table.rows.is_empty() {
            return;
        }
        self.lines.extend(render_table(table, self.width));
    }
}

fn heading_lines(text: &str, level: MarkdownHeadingLevel, width: usize) -> Vec<String> {
    match level {
        MarkdownHeadingLevel::H1 => glyph_heading_lines(text, width, 2),
        MarkdownHeadingLevel::H2 => glyph_heading_lines(text, width, 1),
        MarkdownHeadingLevel::H3 => wrap_words(&letterspaced_heading(&text.to_uppercase()), width),
        MarkdownHeadingLevel::H4 | MarkdownHeadingLevel::H5 | MarkdownHeadingLevel::H6 => {
            wrap_words(text, width)
        }
    }
}

fn heading_rule(level: MarkdownHeadingLevel) -> Option<String> {
    Some(match level {
        MarkdownHeadingLevel::H1 => "━".repeat(32),
        MarkdownHeadingLevel::H2 => "─".repeat(24),
        _ => return None,
    })
}

fn glyph_heading_lines(text: &str, width: usize, scale: usize) -> Vec<String> {
    let glyph_width = 4 * scale;
    let chars_per_line = (width / glyph_width).max(1);
    let mut lines = Vec::new();

    for wrapped in wrap_words(&text.to_uppercase(), chars_per_line) {
        let glyphs = wrapped.chars().map(heading_glyph).collect::<Vec<_>>();
        for row in 0..3 {
            let line = glyphs
                .iter()
                .map(|glyph| scale_glyph_row(glyph[row], scale))
                .collect::<Vec<_>>()
                .join(" ")
                .trim_end()
                .to_string();
            if !line.is_empty() {
                lines.push(line);
            }
        }
    }

    lines
}

fn scale_glyph_row(row: &str, scale: usize) -> String {
    if scale <= 1 {
        return row.to_string();
    }

    row.chars()
        .flat_map(|character| std::iter::repeat_n(character, scale))
        .collect()
}

fn heading_glyph(character: char) -> [&'static str; 3] {
    match character {
        'A' => ["▄▀▄", "█▀█", "▀ ▀"],
        'B' => ["█▀▄", "█▀▄", "▀▀ "],
        'C' => ["█▀▀", "█  ", "▀▀▀"],
        'D' => ["█▀▄", "█ █", "▀▀ "],
        'E' => ["█▀▀", "█▀ ", "▀▀▀"],
        'F' => ["█▀▀", "█▀ ", "▀  "],
        'G' => ["█▀▀", "█ ▄", "▀▀▀"],
        'H' => ["█ █", "█▀█", "▀ ▀"],
        'I' => ["▀█▀", " █ ", "▀▀▀"],
        'J' => [" ▀█", "  █", "▀▀ "],
        'K' => ["█ █", "█▀▄", "▀ ▀"],
        'L' => ["█  ", "█  ", "▀▀▀"],
        'M' => ["█▄█", "█ █", "▀ ▀"],
        'N' => ["█▄█", "█▀█", "▀ ▀"],
        'O' => ["█▀█", "█ █", "▀▀▀"],
        'P' => ["█▀█", "█▀▀", "▀  "],
        'Q' => ["█▀█", "█▄█", "▀▀█"],
        'R' => ["█▀█", "█▀▄", "▀ ▀"],
        'S' => ["█▀▀", "▀▀█", "▀▀▀"],
        'T' => ["▀█▀", " █ ", " ▀ "],
        'U' => ["█ █", "█ █", "▀▀▀"],
        'V' => ["█ █", "█ █", " ▀ "],
        'W' => ["█ █", "█ █", "▀▄▀"],
        'X' => ["█ █", "▄▀▄", "▀ ▀"],
        'Y' => ["█ █", "▀█▀", " ▀ "],
        'Z' => ["▀▀█", "▄▀ ", "▀▀▀"],
        '0' => ["█▀█", "█ █", "▀▀▀"],
        '1' => ["▄█ ", " █ ", "▀▀▀"],
        '2' => ["▀▀█", "█▀▀", "▀▀▀"],
        '3' => ["▀▀█", " ▀█", "▀▀▀"],
        '4' => ["█ █", "▀▀█", "  ▀"],
        '5' => ["█▀▀", "▀▀█", "▀▀▀"],
        '6' => ["█▀▀", "█▀█", "▀▀▀"],
        '7' => ["▀▀█", "  █", "  ▀"],
        '8' => ["█▀█", "█▀█", "▀▀▀"],
        '9' => ["█▀█", "▀▀█", "▀▀▀"],
        '-' | '–' | '—' => ["   ", "▀▀▀", "   "],
        '_' => ["   ", "   ", "▀▀▀"],
        '/' => ["  █", " █ ", "█  "],
        '.' => ["   ", "   ", " ▀ "],
        ':' => [" ▀ ", "   ", " ▀ "],
        '&' => ["█▄ ", "█▄█", "▀▄█"],
        ' ' => ["   ", "   ", "   "],
        _ => ["▀▀█", " ▄▀", " ▀ "],
    }
}

fn letterspaced_heading(text: &str) -> String {
    text.chars()
        .flat_map(|character| [character, ' '])
        .collect::<String>()
        .trim_end()
        .to_string()
}

fn wrap_spans(spans: Vec<Span<'static>>, width: usize) -> Vec<Vec<Span<'static>>> {
    let width = width.max(1);
    let mut lines = vec![Vec::new()];
    let mut current_width = 0;

    for span in spans {
        let style = span.style;
        for segment in span.content.split_inclusive('\n') {
            let hard_break = segment.ends_with('\n');
            let segment = segment.trim_end_matches('\n');
            for word in segment.split_whitespace() {
                let word_width = display_width(word);
                if current_width == 0 {
                    push_wrapped_word(&mut lines, word, style, width, &mut current_width);
                } else if current_width + 1 + word_width <= width {
                    lines
                        .last_mut()
                        .expect("line exists")
                        .push(Span::styled(" ", style));
                    current_width += 1;
                    push_wrapped_word(&mut lines, word, style, width, &mut current_width);
                } else {
                    lines.push(Vec::new());
                    current_width = 0;
                    push_wrapped_word(&mut lines, word, style, width, &mut current_width);
                }
            }
            if hard_break {
                lines.push(Vec::new());
                current_width = 0;
            }
        }
    }

    trim_trailing_empty_line(lines)
}

fn push_wrapped_word(
    lines: &mut Vec<Vec<Span<'static>>>,
    word: &str,
    style: Style,
    width: usize,
    current_width: &mut usize,
) {
    let mut remainder = word;
    while display_width(remainder) > width {
        let chunk: String = remainder.chars().take(width).collect();
        let chunk_len = chunk.len();
        lines
            .last_mut()
            .expect("line exists")
            .push(Span::styled(chunk, style));
        lines.push(Vec::new());
        remainder = &remainder[chunk_len..];
        *current_width = 0;
    }
    if !remainder.is_empty() {
        lines
            .last_mut()
            .expect("line exists")
            .push(Span::styled(remainder.to_string(), style));
        *current_width += display_width(remainder);
    }
}

fn trim_trailing_empty_line(mut lines: Vec<Vec<Span<'static>>>) -> Vec<Vec<Span<'static>>> {
    if lines.len() > 1 && lines.last().is_some_and(Vec::is_empty) {
        lines.pop();
    }
    lines
}

fn spans_plain_text(spans: &[Span<'_>]) -> String {
    spans.iter().map(|span| span.content.as_ref()).collect()
}

fn quote_prefix(depth: usize) -> String {
    "│ ".repeat(depth)
}

fn render_table(table: TableState, width: usize) -> Vec<Line<'static>> {
    let columns = table.rows.iter().map(Vec::len).max().unwrap_or(0);
    if columns == 0 {
        return Vec::new();
    }

    let mut widths = vec![3usize; columns];
    for row in &table.rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(spans_plain_text(cell).chars().count().min(32));
        }
    }

    let chrome = columns.saturating_add(1);
    let separators = columns.saturating_sub(1) * 3;
    let available = width.saturating_sub(chrome + separators).max(columns);
    let total: usize = widths.iter().sum();
    if total > available {
        for column_width in &mut widths {
            *column_width = ((*column_width * available) / total).max(3);
        }
    }

    let mut lines = Vec::new();
    lines.push(table_border('┌', '┬', '┐', &widths));
    for (row_index, row) in table.rows.iter().enumerate() {
        lines.push(table_row(row, &widths, &table.alignments));
        if row_index == 0 {
            lines.push(table_border('├', '┼', '┤', &widths));
        }
    }
    lines.push(table_border('└', '┴', '┘', &widths));
    lines
}

fn table_border(left: char, separator: char, right: char, widths: &[usize]) -> Line<'static> {
    let mut text = String::new();
    text.push(left);
    for (index, width) in widths.iter().enumerate() {
        if index > 0 {
            text.push(separator);
        }
        text.push_str(&"─".repeat(*width + 2));
    }
    text.push(right);
    Line::from(Span::styled(text, fg(RULE_COLOR)))
}

fn table_row(
    row: &[Vec<Span<'static>>],
    widths: &[usize],
    alignments: &[MarkdownAlignment],
) -> Line<'static> {
    let mut spans = vec![Span::styled("│", fg(RULE_COLOR))];
    for (index, width) in widths.iter().enumerate() {
        let text = row
            .get(index)
            .map(|cell| spans_plain_text(cell))
            .unwrap_or_default();
        let text = truncate_flat(&text, *width);
        let text_width = display_width(&text);
        let remaining = width.saturating_sub(text_width);
        let (left_pad, right_pad) = match alignments.get(index).unwrap_or(&MarkdownAlignment::None)
        {
            MarkdownAlignment::Right => (remaining, 0),
            MarkdownAlignment::Center => (remaining / 2, remaining - remaining / 2),
            MarkdownAlignment::Left | MarkdownAlignment::None => (0, remaining),
        };
        spans.push(Span::raw(format!(
            " {}{}{} ",
            " ".repeat(left_pad),
            text,
            " ".repeat(right_pad)
        )));
        spans.push(Span::styled("│", fg(RULE_COLOR)));
    }
    Line::from(spans)
}
