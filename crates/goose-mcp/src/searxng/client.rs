//! Low-level SearXNG client with JSON + HTML fallback and fast-fail semantics.
//!
//! Designed for concurrent use: many instances are queried at once.
//! Bad instances (403, 429, non-JSON, malformed, timeouts) are dropped quickly.
//!
//! Optional third path: FlareSolverr rendered fetch (when configured).
//! Only attempted per-backend after direct JSON + HTML fail.
//! Long timeout, does not block other parallel backends.

use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub title: Option<String>,
    pub url: String,
    pub snippet: Option<String>,
    pub engines: Vec<String>,
    pub score: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct SearxngJson {
    #[serde(default)]
    results: Vec<SearxngResultItem>,
}

#[derive(Debug, Deserialize)]
struct SearxngResultItem {
    title: Option<String>,
    url: Option<String>,
    content: Option<String>,
    #[serde(default)]
    engine: Option<String>,
    #[serde(default)]
    engines: Vec<String>,
    score: Option<f64>,
}

/// Try one SearXNG instance.
/// Fast-fail: returns None quickly on errors instead of hanging.
///
/// Paths (in order):
/// 1. JSON (fast)
/// 2. Plain HTML + simple theme parser (fallback)
/// 3. Optional FlareSolverr rendered fetch (last resort, only if SEARXNG_FLARESOLVERR_URL set
///    and the first two produced no results for *this* backend).
///
/// The FS path is intentionally slow (up to ~90s) and per-backend. Because each backend
/// runs in its own task in parallel_search_stream, a slow FS call for one provider
/// does not block the others. This keeps the "full parallel, no limit" behavior intact.
pub async fn search_one(
    client: &Client,
    base_url: &str,
    query: &str,
    language: &str,
) -> Option<SearchResultSet> {
    let base = base_url.trim_end_matches('/');
    let url = format!("{}/search", base);

    // Attempt 1: JSON (preferred)
    if let Some(set) = try_json(client, &url, query, language).await {
        if !set.results.is_empty() {
            return Some(set);
        }
    }

    // Attempt 2: HTML fallback (many public instances block format=json)
    if let Some(set) = try_html_fallback(client, &url, query, language).await {
        if !set.results.is_empty() {
            return Some(set);
        }
    }

    // Attempt 3 (optional, non-blocking for the set): FlareSolverr
    // Only for backends that "need it" (direct paths failed to yield results).
    // Other parallel backends proceed independently.
    if let Some(fs_base) = flaresolverr_url() {
        if let Some(set) = try_flaresolverr(client, &fs_base, &url, query, language).await {
            if !set.results.is_empty() {
                return Some(set);
            }
        }
    }

    None
}

fn flaresolverr_url() -> Option<String> {
    std::env::var("SEARXNG_FLARESOLVERR_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim_end_matches('/').to_string())
}

#[derive(Debug, Clone)]
pub struct SearchResultSet {
    pub backend: String,
    pub results: Vec<SearchResult>,
}

async fn try_json(
    client: &Client,
    url: &str,
    query: &str,
    language: &str,
) -> Option<SearchResultSet> {
    let resp = client
        .get(url)
        .query(&[
            ("q", query),
            ("format", "json"),
            ("language", language),
            ("safesearch", "0"),
        ])
        .timeout(Duration::from_secs(6))
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None; // fast fail
    }

    let json: SearxngJson = resp.json().await.ok()?;

    let results: Vec<SearchResult> = json
        .results
        .into_iter()
        .filter_map(|r| {
            let url = r.url?;
            if url.is_empty() {
                return None;
            }
            let mut engines = r.engines;
            if engines.is_empty() {
                if let Some(e) = r.engine {
                    engines.push(e);
                }
            }
            Some(SearchResult {
                title: r.title,
                url,
                snippet: r.content,
                engines,
                score: r.score,
            })
        })
        .collect();

    if results.is_empty() {
        return None;
    }

    Some(SearchResultSet {
        backend: url.to_string(),
        results,
    })
}

/// Very lightweight HTML fallback.
/// Parses the common "simple" theme structure.
/// Not perfect, but good enough for many public instances when JSON is blocked.
async fn try_html_fallback(
    client: &Client,
    url: &str,
    query: &str,
    language: &str,
) -> Option<SearchResultSet> {
    let resp = client
        .get(url)
        .query(&[("q", query), ("language", language), ("safesearch", "0")])
        .header("Accept", "text/html,application/xhtml+xml")
        .timeout(Duration::from_secs(6))
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let html = resp.text().await.ok()?;
    let results = parse_simple_theme_html(&html);

    if results.is_empty() {
        None
    } else {
        Some(SearchResultSet {
            backend: url.to_string(),
            results,
        })
    }
}

/// Reusable parser for the common SearXNG "simple" theme HTML.
/// Used by both direct HTML fallback and FlareSolverr rendered path.
fn parse_simple_theme_html(html: &str) -> Vec<SearchResult> {
    let mut results = Vec::new();

    for block in html.split("<article class=\"result") {
        if let Some(after) = block.split("</article>").next() {
            if let Some(href_start) = after.find("href=\"") {
                let start = href_start + 6;
                if let Some(rest) = after.get(start..) {
                    if let Some(href_end) = rest.find('"') {
                        if let Some(href) = rest.get(..href_end) {
                            if href.starts_with("http") {
                                let title = after
                                    .split("<h3")
                                    .nth(1)
                                    .and_then(|s| s.split("</h3>").next())
                                    .and_then(|s| {
                                        // Extract visible text: split on > and take the first non-empty
                                        // segment that is not an attribute or tag opener.
                                        s.split('>')
                                            .filter_map(|p| {
                                                let t = p.split('<').next().unwrap_or(p).trim();
                                                if t.is_empty()
                                                    || t.contains("href=")
                                                    || t.contains("http")
                                                    || t.starts_with('/')
                                                {
                                                    None
                                                } else {
                                                    Some(t.to_string())
                                                }
                                            })
                                            .next()
                                    });

                                let snippet = after
                                    .split("class=\"content")
                                    .nth(1)
                                    .and_then(|s| s.split("</p>").next())
                                    .map(|s| {
                                        s.split('>').next_back().unwrap_or("").trim().to_string()
                                    })
                                    .filter(|s| !s.is_empty());

                                results.push(SearchResult {
                                    title,
                                    url: href.to_string(),
                                    snippet,
                                    engines: vec!["html-fallback".to_string()],
                                    score: None,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    results
}

/// Last-resort rendered fetch via FlareSolverr.
/// Only called when direct JSON and direct HTML both failed to produce results for this backend.
///
/// This call can take a long time (FlareSolverr spins Chrome and may wait for challenges).
/// Because it runs inside its own tokio task per-backend (see parallel_search_stream),
/// a slow FS solve for one provider does **not** block the other providers or the merge stream.
///
/// Returns None on any error or if no usable results after parsing.
async fn try_flaresolverr(
    client: &Client,
    fs_base: &str,
    search_url: &str,
    _query: &str,
    _language: &str,
) -> Option<SearchResultSet> {
    let fs_endpoint = format!("{}/v1", fs_base.trim_end_matches('/'));

    // Tell FlareSolverr to fetch the full search URL (it will render + solve challenges).
    let body = json!({
        "cmd": "request.get",
        "url": search_url,
        "maxTimeout": 90000u64,
        "waitInSeconds": 1u64,
        // We could add "session" for reuse in the future.
    });

    // Use a long per-request timeout; the client default (8s) is too short for rendered solves.
    // Per-request timeout overrides the client one in reqwest.
    let resp = client
        .post(&fs_endpoint)
        .json(&body)
        .timeout(Duration::from_secs(95))
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let value: serde_json::Value = resp.json().await.ok()?;

    // FlareSolverr success shape: { "solution": { "response": "<html>...", "status": 200, ... } }
    let solution = value.get("solution")?;
    let status = solution.get("status").and_then(|s| s.as_i64()).unwrap_or(0);
    if status != 200 {
        return None;
    }

    let html = solution.get("response").and_then(|r| r.as_str())?;
    if html.trim().is_empty() {
        return None;
    }

    let mut results = parse_simple_theme_html(html);

    if results.is_empty() {
        return None;
    }

    // Tag results as coming from rendered fallback.
    for r in &mut results {
        r.engines = vec!["flaresolverr".to_string()];
    }

    Some(SearchResultSet {
        backend: search_url.to_string(),
        results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_theme_html_basic() {
        let html = r#"
            <article class="result">
                <h3><a href="https://example.com/rust">Rust Lang</a></h3>
                <p class="content">Systems programming language.</p>
            </article>
            <article class="result">
                <h3><a href="https://example.com/cargo">Cargo</a></h3>
                <p class="content">Rust package manager.</p>
            </article>
        "#;

        let results = parse_simple_theme_html(html);
        assert_eq!(results.len(), 2);

        assert_eq!(results[0].url, "https://example.com/rust");
        assert_eq!(results[0].title.as_deref(), Some("Rust Lang"));
        assert_eq!(
            results[0].snippet.as_deref(),
            Some("Systems programming language.")
        );

        assert_eq!(results[1].url, "https://example.com/cargo");
        assert_eq!(results[1].title.as_deref(), Some("Cargo"));
        assert_eq!(results[1].snippet.as_deref(), Some("Rust package manager."));
    }

    #[test]
    fn test_parse_simple_theme_html_empty() {
        let results = parse_simple_theme_html("<html><body>no results here</body></html>");
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_simple_theme_html_ignores_non_http() {
        let html = r#"<article class="result"><h3><a href="/relative">Bad</a></h3></article>"#;
        let results = parse_simple_theme_html(html);
        assert!(results.is_empty());
    }
}
