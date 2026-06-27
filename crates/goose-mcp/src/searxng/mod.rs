/*!
 * SearXNG MCP extension for goose.
 *
 * Secret sauce: when free providers are enabled (default), we **always** use
 * **all** configured free providers in full parallel (no concurrency limit whatsoever),
 * with fast-fail + HTML fallback + merge.
 *
 * Only the 8 verified working providers are in DEFAULT_FREE_PROVIDERS.
 * Dead/unreliable providers are tracked only in docs/searxng-mcp/DEAD_PROVIDERS.md
 * (never mixed into active code or defaults).
 */

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, Content, ErrorData, Implementation, InitializeResult,
        ServerCapabilities, ServerInfo,
    },
    schemars::JsonSchema,
    tool, tool_handler, tool_router, ServerHandler,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Semaphore;
use reqwest::Client;
use std::time::Duration;

pub mod client;
pub mod merge;

use client::{search_one, SearchResultSet};
use merge::merge_results;

/// Verified working free public SearXNG providers.
/// These are **always** used in full parallel (no limit) + fast-fail + merge.
pub const DEFAULT_FREE_PROVIDERS: &[&str] = &[
    "https://searx.tiekoetter.com",
    "https://baresearch.org",
    "https://search.2b9t.xyz",
    "https://search.abohiccups.com",
    "https://searxng.site",
    "https://failsearx.culturanerd.it",
    "https://searx.prvcy.eu",
    "https://search.bladerunn.in",
];

/// Parameters for searxng_search
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SearxngSearchParams {
    pub query: String,
    #[serde(default = "default_lang")]
    pub language: String,
    #[serde(default = "default_max_results")]
    pub max_results: u32,
}

fn default_lang() -> String { "en".to_string() }
fn default_max_results() -> u32 { 10 }

#[derive(Clone)]
pub struct SearxngServer {
    tool_router: ToolRouter<Self>,
    http: Client,
    free_providers: Vec<String>,
}

impl Default for SearxngServer {
    fn default() -> Self { Self::new() }
}

impl SearxngServer {
    pub fn new() -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(8))
            .user_agent("goose-searxng/0.1")
            .build()
            .expect("failed to build http client");

        // Free providers come from env or the verified working default list.
        // We always run the entire list in full parallel (no cap).
        let free_providers: Vec<String> = std::env::var("SEARXNG_FREE_PROVIDERS")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.split(',').map(|p| p.trim().trim_end_matches('/').to_string()).collect())
            .unwrap_or_else(|| DEFAULT_FREE_PROVIDERS.iter().map(|s| s.to_string()).collect());

        Self {
            tool_router: ToolRouter::new(),
            http,
            free_providers,
        }
    }

    /// Always the full configured free provider list (plus optional base) in parallel.
    /// No artificial limit on the free pool.
    async fn parallel_search(&self, query: &str, language: &str) -> Vec<merge::MergedResult> {
        let mut targets: Vec<String> = vec![];

        if let Ok(base) = std::env::var("SEARXNG_BASE_URL") {
            if !base.trim().is_empty() {
                targets.push(base.trim_end_matches('/').to_string());
            }
        }

        // Always the full list — this is the key "all always" behavior.
        targets.extend(self.free_providers.clone());

        if targets.is_empty() {
            // Absolute fallback still uses the full verified working list.
            targets = DEFAULT_FREE_PROVIDERS.iter().map(|s| s.to_string()).collect();
        }

        // One permit per target → full parallel, no cap for free providers.
        let sem = Arc::new(Semaphore::new(targets.len().max(1)));
        let client = self.http.clone();
        let lang = language.to_string();
        let q = query.to_string();

        let mut handles = vec![];
        for base in targets {
            let sem = sem.clone();
            let client = client.clone();
            let lang = lang.clone();
            let q = q.clone();

            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.ok();
                search_one(&client, &base, &q, &lang).await
            }));
        }

        let mut outcomes: Vec<SearchResultSet> = vec![];
        for h in handles {
            if let Ok(Some(r)) = h.await {
                if !r.results.is_empty() {
                    outcomes.push(r);
                }
            }
        }

        merge_results(outcomes)
    }
}

#[tool_router(router = tool_router)]
impl SearxngServer {
    #[tool(
        description = "SearXNG search. All configured free providers are always run in full parallel (no limit) with fast-fail + HTML fallback + merge. Only verified working providers by default."
    )]
    async fn searxng_search(
        &self,
        Parameters(params): Parameters<SearxngSearchParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let results = self.parallel_search(&params.query, &params.language).await;
        let limited: Vec<_> = results.into_iter().take(params.max_results as usize).collect();

        let text = if limited.is_empty() {
            format!("No results for '{}'", params.query)
        } else {
            let mut lines = vec![
                format!("Query: {}", params.query),
                format!("Free providers (full parallel, no limit): {}", self.free_providers.len()),
                "".to_string(),
            ];
            for (i, r) in limited.iter().enumerate() {
                let title = r.title.as_deref().unwrap_or(&r.url);
                lines.push(format!(
                    "{}. {} — {} {}",
                    i + 1,
                    title,
                    r.url,
                    if !r.engines.is_empty() { format!("(via {})", r.engines.join(",")) } else { "".into() }
                ));
                if let Some(sn) = &r.snippet {
                    lines.push(format!("   {}", sn));
                }
            }
            lines.join("\n")
        };

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}

#[tool_handler]
impl ServerHandler for SearxngServer {
    fn get_info(&self) -> ServerInfo {
        InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("searxng", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "SearXNG metasearch. All configured free providers are always used in full parallel (no limit), \
                 with fast-fail + HTML fallback + merge. \
                 Only the 8 verified working providers are in DEFAULT_FREE_PROVIDERS (dead list is only in docs). \
                 Dead/unreliable providers are tracked only in docs/searxng-mcp/DEAD_PROVIDERS.md."
            )
    }
}
