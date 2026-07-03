//! Secret-detection heuristic used to gate `file_system_read_file`, matching
//! upstream repomix's own "security scanning is always on" behavior for that
//! tool. Not a shared crate dependency (goose-mcp doesn't depend on goose) —
//! this is a small self-contained port of the entropy heuristic originally
//! written for session-diagnostics redaction.

use std::collections::HashMap;

const ENTROPY_THRESHOLD: f64 = 3.5;
const MIN_SECRET_LEN: usize = 20;

/// Shannon entropy in bits per character.
fn shannon_entropy(s: &str) -> f64 {
    let len = s.len() as f64;
    if len == 0.0 {
        return 0.0;
    }
    let mut counts: HashMap<u8, usize> = HashMap::new();
    for &b in s.as_bytes() {
        *counts.entry(b).or_default() += 1;
    }
    counts
        .values()
        .map(|&c| {
            let p = c as f64 / len;
            -p * p.log2()
        })
        .sum()
}

/// Returns true if `token` looks like a secret based on length, entropy, and
/// character composition. Secrets (API keys, tokens) are long, high-entropy
/// strings composed almost entirely of alphanumeric chars, hyphens, and
/// underscores. Non-secrets like URLs, paths, and natural-language text
/// contain structural characters (slashes, spaces, colons, etc.).
fn looks_like_secret(token: &str) -> bool {
    if token.len() < MIN_SECRET_LEN {
        return false;
    }
    if shannon_entropy(token) <= ENTROPY_THRESHOLD {
        return false;
    }
    if token.bytes().any(|b| {
        matches!(
            b,
            b' ' | b'/'
                | b':'
                | b'@'
                | b','
                | b';'
                | b'!'
                | b'?'
                | b'('
                | b')'
                | b'['
                | b']'
                | b'{'
                | b'}'
                | b'='
                | b'&'
                | b'+'
                | b'#'
        )
    }) {
        return false;
    }
    // Dots need special handling: JWTs (header.payload.signature) are secrets,
    // but hostnames (api.openai.com) and versions (6.2.9200) are not.
    if token.contains('.') {
        let parts: Vec<&str> = token.split('.').collect();
        return parts.len() == 3 && parts.iter().all(|p| p.len() >= 4);
    }
    true
}

/// Scans file content for any token that looks like a secret. Returns a
/// redacted preview of the first match found, or `None` if the content looks
/// clean. Used to refuse serving a file's content outright, mirroring
/// upstream repomix's `file_system_read_file` security validation.
pub fn detect_secret(content: &str) -> Option<String> {
    for line in content.lines() {
        // Split on '=' and ':' too so "KEY=value"/"key: value" style lines
        // check the value on its own, not the whole "key=value" token (which
        // would otherwise be excluded by looks_like_secret's '=' guard).
        for raw_word in
            line.split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '=' | ':'))
        {
            let word = raw_word
                .trim_matches(|c: char| matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';'));
            if looks_like_secret(word) {
                return Some(redacted_preview(word));
            }
        }
    }
    None
}

fn redacted_preview(token: &str) -> String {
    if token.len() <= 8 {
        "[REDACTED]".to_string()
    } else {
        let prefix: String = token.chars().take(4).collect();
        format!("{prefix}…[REDACTED]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_high_entropy_api_key() {
        // Fake, non-functional key shaped like a real one; split so the contiguous
        // pattern never appears in source (avoids secret-scanner false positives).
        let content = format!(
            "OPENAI_API_KEY={}\n",
            format!("sk-proj-{}", "aBcDeFgHiJkLmNoPqRsTuVwXyZ1234567890")
        );
        assert!(detect_secret(&content).is_some());
    }

    #[test]
    fn ignores_urls_and_paths() {
        let content = "See https://api.openai.com/v1/chat/completions for docs.\n\
                        path: /home/user/some/very/long/directory/structure/here\n";
        assert!(detect_secret(content).is_none());
    }

    #[test]
    fn ignores_normal_code() {
        let content = "fn main() {\n    println!(\"Hello, world! This is normal code.\");\n}\n";
        assert!(detect_secret(content).is_none());
    }

    #[test]
    fn detects_jwt_shaped_token() {
        let content = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c\n";
        assert!(detect_secret(content).is_some());
    }

    #[test]
    fn ignores_hostnames_and_version_strings() {
        assert!(!looks_like_secret("api.openai.com"));
        assert!(!looks_like_secret("6.2.9200.12345"));
    }

    #[test]
    fn ignores_short_tokens() {
        assert!(!looks_like_secret("short"));
    }

    #[test]
    fn redacted_preview_keeps_short_prefix() {
        let preview = redacted_preview("abcdefghijklmnopqrstuvwxyz1234567890");
        assert!(preview.starts_with("abcd"));
        assert!(preview.ends_with("[REDACTED]"));
    }
}
