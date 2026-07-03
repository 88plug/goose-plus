//! Parses the human-readable summary lines repomix prints to stdout after a
//! successful pack (see upstream `src/cli/cliReport.ts`: `"  Total Files: {n}
//! files"`, `" Total Tokens: {n} tokens"`, `"  Total Chars: {n} chars"`).
//!
//! `pack_codebase`/`pack_remote_repository` are run without `--quiet`
//! specifically so these lines are emitted, giving exact parity with
//! repomix's own file/character/token counts (including its real tokenizer)
//! rather than a Rust-side re-estimate. If the summary format ever changes
//! upstream, `parse_summary` degrades gracefully — a pack must never fail
//! just because its own summary couldn't be parsed.

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct PackMetrics {
    pub total_files: u64,
    pub total_tokens: u64,
    pub total_characters: u64,
}

pub fn parse_summary(stdout: &str) -> PackMetrics {
    let mut metrics = PackMetrics::default();
    for line in stdout.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Total Files:") {
            metrics.total_files = parse_leading_number(rest);
        } else if let Some(rest) = line.strip_prefix("Total Tokens:") {
            metrics.total_tokens = parse_leading_number(rest);
        } else if let Some(rest) = line.strip_prefix("Total Chars:") {
            metrics.total_characters = parse_leading_number(rest);
        }
    }
    metrics
}

fn parse_leading_number(s: &str) -> u64 {
    let cleaned: String = s
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == ',')
        .collect();
    cleaned.replace(',', "").parse().unwrap_or(0)
}

/// Best-effort metrics computed directly from the packed output file, used
/// only when `parse_summary` found nothing (e.g. upstream changed its
/// wording). Token count is a rough characters/4 estimate, not repomix's
/// real tokenizer — clearly a fallback, not a replacement.
pub fn fallback_metrics(content: &str) -> PackMetrics {
    let total_characters = content.chars().count() as u64;
    let total_files = content.matches("<file path=").count() as u64;
    PackMetrics {
        total_files,
        total_tokens: total_characters / 4,
        total_characters,
    }
}

/// Parses the summary, falling back to file-derived estimates if the summary
/// carried no recognizable totals at all.
pub fn extract_metrics(stdout: &str, output_content: &str) -> PackMetrics {
    let parsed = parse_summary(stdout);
    if parsed == PackMetrics::default() {
        fallback_metrics(output_content)
    } else {
        parsed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_summary_format() {
        let stdout = "📦 Repomix v1.14.1\n\n\
             📊 Pack Summary:\n\
             ────────────────\n\
             \n  Total Files: 42 files\n \
             Total Tokens: 12,345 tokens\n\
             \n  Total Chars: 98,765 chars\n";
        let metrics = parse_summary(stdout);
        assert_eq!(metrics.total_files, 42);
        assert_eq!(metrics.total_tokens, 12345);
        assert_eq!(metrics.total_characters, 98765);
    }

    #[test]
    fn falls_back_when_summary_unrecognized() {
        let stdout = "some unrelated log output\nnothing matches here\n";
        let content = "<file path=\"a.rs\">fn a() {}</file><file path=\"b.rs\">fn b() {}</file>";
        let metrics = extract_metrics(stdout, content);
        assert_eq!(metrics.total_files, 2);
        assert_eq!(metrics.total_characters, content.chars().count() as u64);
        assert!(metrics.total_tokens > 0);
    }

    #[test]
    fn prefers_parsed_summary_over_fallback() {
        let stdout = "  Total Files: 3 files\n Total Tokens: 10 tokens\n  Total Chars: 40 chars\n";
        let content = "<file path=\"a.rs\"></file>";
        let metrics = extract_metrics(stdout, content);
        assert_eq!(metrics.total_files, 3);
        assert_eq!(metrics.total_tokens, 10);
        assert_eq!(metrics.total_characters, 40);
    }
}
