//! Result merging logic for multi-backend SearXNG searches.
//!
//! Port of the Python `_merge_query_outcomes` / MergedHit behavior.
//! - Deduplicate by canonical URL
//! - Aggregate engines
//! - Boost by number of hits across backends
//! - Preserve best title/snippet

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergedResult {
    pub url: String,
    pub title: Option<String>,
    pub snippet: Option<String>,
    pub engines: Vec<String>,
    pub hit_count: usize,
    pub score: f64,
}

pub fn merge_results(sets: Vec<super::client::SearchResultSet>) -> Vec<MergedResult> {
    let mut groups: HashMap<String, MergedResult> = HashMap::new();

    for set in sets {
        for raw in set.results {
            let canonical = normalize_url(&raw.url);

            let entry = groups
                .entry(canonical.clone())
                .or_insert_with(|| MergedResult {
                    url: raw.url.clone(),
                    title: raw.title.clone(),
                    snippet: raw.snippet.clone(),
                    engines: vec![],
                    hit_count: 0,
                    score: raw.score.unwrap_or(0.0),
                });

            // Prefer longer/better title and snippet
            if entry.title.as_ref().map_or(0, |t| t.len())
                < raw.title.as_ref().map_or(0, |t| t.len())
            {
                entry.title = raw.title.clone();
            }
            if entry.snippet.as_ref().map_or(0, |t| t.len())
                < raw.snippet.as_ref().map_or(0, |t| t.len())
            {
                entry.snippet = raw.snippet.clone();
            }

            entry.hit_count += 1;

            // Merge engines
            let new_engines = raw.engines.clone();
            for e in new_engines {
                if !entry.engines.contains(&e) {
                    entry.engines.push(e);
                }
            }

            if let Some(sc) = raw.score {
                if sc > entry.score {
                    entry.score = sc;
                }
            }
        }
    }

    let mut merged: Vec<MergedResult> = groups.into_values().collect();

    // Sort: more hits first, then higher score, then url
    merged.sort_by(|a, b| {
        b.hit_count
            .cmp(&a.hit_count)
            .then(
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(a.url.cmp(&b.url))
    });

    merged
}

fn normalize_url(u: &str) -> String {
    if let Ok(parsed) = Url::parse(u) {
        let mut s = parsed.scheme().to_string() + "://" + parsed.host_str().unwrap_or("");
        if let Some(port) = parsed.port() {
            if !(parsed.scheme() == "http" && port == 80)
                && !(parsed.scheme() == "https" && port == 443)
            {
                s.push(':');
                s.push_str(&port.to_string());
            }
        }
        s.push_str(parsed.path());
        if let Some(query) = parsed.query() {
            // drop tracking params for better dedup
            let cleaned: Vec<_> = query
                .split('&')
                .filter(|p| {
                    !p.starts_with("utm_") && !p.starts_with("fbclid") && !p.starts_with("gclid")
                })
                .collect();
            if !cleaned.is_empty() {
                s.push('?');
                s.push_str(&cleaned.join("&"));
            }
        }
        s
    } else {
        u.to_string()
    }
}
