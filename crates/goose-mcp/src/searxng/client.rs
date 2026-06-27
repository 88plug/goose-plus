//! Low-level SearXNG client with JSON + HTML fallback and fast-fail semantics.
//!
//! Designed for concurrent use: many instances are queried at once.
//! Bad instances (403, 429, non-JSON, malformed, timeouts) are dropped quickly.

use reqwest::Client;
use serde::Deserialize;
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

    None
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

    // Very simple extraction for the common SearXNG simple theme.
    // Looks for <article class="result ..."> blocks.
    let mut results = Vec::new();

    // crude but effective for the documented structure
    for block in html.split("<article class=\"result") {
        if let Some(after) = block.split("</article>").next() {
            // Find first <a ... href="..."> that looks like a result
            if let Some(href_start) = after.find("href=\"") {
                let rest = &after[href_start + 6..];
                if let Some(href_end) = rest.find('"') {
                    let href = &rest[..href_end];
                    if href.starts_with("http") {
                        // title is usually in the following <h3>
                        let title = after
                            .split("<h3")
                            .nth(1)
                            .and_then(|s| s.split("</h3>").next())
                            .map(|s| s.split('>').last().unwrap_or("").trim().to_string())
                            .filter(|t| !t.is_empty());

                        // snippet from .content or first <p>
                        let snippet = after
                            .split("class=\"content")
                            .nth(1)
                            .and_then(|s| s.split("</p>").next())
                            .map(|s| s.split('>').last().unwrap_or("").trim().to_string())
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

    if results.is_empty() {
        None
    } else {
        Some(SearchResultSet {
            backend: url.to_string(),
            results,
        })
    }
}
