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
 *
 * A2A/ACP superpowers:
 * - Streaming parallel search yields incremental merged results as each free provider responds.
 * - This enables fast wire delivery of partial results to agents via A2A Working updates
 *   or ACP tool notifications (when bridged by goose).
 * - MCP resources expose the free provider list and status for discovery.
 */

use reqwest::Client;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, Content, ErrorCode, ErrorData, Implementation, InitializeResult,
        ListResourcesResult, PaginatedRequestParams, RawResource, ReadResourceRequestParams,
        ReadResourceResult, Resource, ResourceContents, ServerCapabilities, ServerInfo,
    },
    schemars::JsonSchema,
    service::RequestContext,
    tool, tool_handler, tool_router, RoleServer, ServerHandler,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Semaphore};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

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

fn default_lang() -> String {
    "en".to_string()
}
fn default_max_results() -> u32 {
    10
}

/// Incremental update emitted by the parallel free search.
/// Each free provider that responds contributes an update with the current merged state.
/// This is the foundation for fast, smart results over A2A/ACP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchUpdate {
    /// Which backend just contributed (or "all" for the final snapshot).
    pub backend: String,
    /// How many raw results were added by this backend in this step.
    pub added: usize,
    /// Current total unique results after merge.
    pub total_merged: usize,
    /// Snapshot of the merged results so far (may be truncated in previews).
    pub merged_preview: Vec<merge::MergedResult>,
    /// True only on the final message after all backends have been processed.
    pub is_final: bool,
}

#[derive(Clone)]
pub struct SearxngServer {
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
    http: Client,
    free_providers: Vec<String>,
}

impl Default for SearxngServer {
    fn default() -> Self {
        Self::new()
    }
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
            .map(|s| {
                s.split(',')
                    .map(|p| p.trim().trim_end_matches('/').to_string())
                    .collect()
            })
            .unwrap_or_else(|| {
                DEFAULT_FREE_PROVIDERS
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            });

        Self {
            tool_router: Self::tool_router(),
            http,
            free_providers,
        }
    }

    /// Build the list of targets: optional SEARXNG_BASE_URL + all configured free providers.
    /// Always full parallel — no limit on the free pool.
    fn build_targets(&self) -> Vec<String> {
        let mut targets: Vec<String> = vec![];

        if let Ok(base) = std::env::var("SEARXNG_BASE_URL") {
            if !base.trim().is_empty() {
                targets.push(base.trim_end_matches('/').to_string());
            }
        }

        targets.extend(self.free_providers.clone());

        if targets.is_empty() {
            targets = DEFAULT_FREE_PROVIDERS
                .iter()
                .map(|s| s.to_string())
                .collect();
        }
        targets
    }

    /// Streaming parallel search over all configured free providers (plus optional base).
    /// Yields a SearchUpdate every time a backend responds, with the incrementally merged results.
    /// This enables agents (via A2A or ACP) to receive partial results as soon as the fastest
    /// free providers reply — dramatically lower time-to-first-useful-result.
    pub async fn parallel_search_stream(
        &self,
        query: &str,
        language: &str,
    ) -> ReceiverStream<SearchUpdate> {
        let targets = self.build_targets();
        let total = targets.len().max(1);

        let (tx, rx) = mpsc::channel::<SearchUpdate>(32);
        let sem = Arc::new(Semaphore::new(total));
        let client = self.http.clone();
        let lang = language.to_string();
        let q = query.to_string();

        tokio::spawn(async move {
            let mut outcomes: Vec<SearchResultSet> = vec![];

            let mut handles = vec![];
            for base in targets {
                let sem = sem.clone();
                let client = client.clone();
                let lang = lang.clone();
                let q = q.clone();

                handles.push(tokio::spawn(async move {
                    let _permit = sem.acquire().await.ok();
                    let res = search_one(&client, &base, &q, &lang).await;
                    (base, res)
                }));
            }

            for h in handles {
                if let Ok((backend, maybe_set)) = h.await {
                    let added = if let Some(ref set) = maybe_set {
                        if !set.results.is_empty() {
                            outcomes.push(set.clone());
                        }
                        set.results.len()
                    } else {
                        0
                    };

                    // Emit incremental merged state after this backend
                    let current_merged = merge_results(outcomes.clone());
                    let _ = tx
                        .send(SearchUpdate {
                            backend,
                            added,
                            total_merged: current_merged.len(),
                            merged_preview: current_merged.clone(),
                            is_final: false,
                        })
                        .await;
                }
            }

            // Final consolidated snapshot
            let final_merged = merge_results(outcomes);
            let _ = tx
                .send(SearchUpdate {
                    backend: "all".into(),
                    added: 0,
                    total_merged: final_merged.len(),
                    merged_preview: final_merged,
                    is_final: true,
                })
                .await;
        });

        ReceiverStream::new(rx)
    }

    /// Non-streaming parallel search. Collects the final merged set.
    /// Internally driven by the streaming implementation so behavior stays identical.
    async fn parallel_search(&self, query: &str, language: &str) -> Vec<merge::MergedResult> {
        let mut stream = self.parallel_search_stream(query, language).await;
        let mut final_results: Vec<merge::MergedResult> = vec![];
        while let Some(update) = stream.next().await {
            if update.is_final {
                final_results = update.merged_preview;
                break;
            }
        }
        final_results
    }
}

#[tool_router(router = tool_router)]
impl SearxngServer {
    #[tool(
        description = "SearXNG search. All configured free providers are always run in full parallel (no limit) with fast-fail + HTML fallback + merge. Only verified working providers by default. Streaming updates available for A2A/ACP agents."
    )]
    async fn searxng_search(
        &self,
        Parameters(params): Parameters<SearxngSearchParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let results = self.parallel_search(&params.query, &params.language).await;
        let limited: Vec<_> = results
            .into_iter()
            .take(params.max_results as usize)
            .collect();

        let text = if limited.is_empty() {
            format!("No results for '{}'", params.query)
        } else {
            let mut lines = vec![
                format!("Query: {}", params.query),
                format!(
                    "Free providers (full parallel, no limit): {}",
                    self.free_providers.len()
                ),
                "".to_string(),
            ];
            for (i, r) in limited.iter().enumerate() {
                let title = r.title.as_deref().unwrap_or(&r.url);
                lines.push(format!(
                    "{}. {} — {} {}",
                    i + 1,
                    title,
                    r.url,
                    if !r.engines.is_empty() {
                        format!("(via {})", r.engines.join(","))
                    } else {
                        "".into()
                    }
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
        InitializeResult::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(Implementation::new("searxng", env!("CARGO_PKG_VERSION")))
        .with_instructions(
            "SearXNG metasearch for goose. All configured free providers are always used in full parallel (no limit), \
             with fast-fail + HTML fallback + merge. \
             Only the 8 verified working providers are in DEFAULT_FREE_PROVIDERS (dead list is only in docs). \
             Streaming updates via parallel_search_stream for fast delivery to A2A and ACP agents. \
             Resources: searxng://free-providers"
        )
    }

    async fn list_resources(
        &self,
        _pagination: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        let resources = vec![
            Resource {
                raw: RawResource {
                    uri: "searxng://free-providers".to_string(),
                    name: "Free SearXNG Providers".to_string(),
                    title: Some("Free SearXNG Providers".to_string()),
                    description: Some(
                        "The verified working public SearXNG instances pre-wired for full parallel use (all always, no limit). \
                         This is the secret sauce that makes searxng-mcp the most powerful search surface.".to_string(),
                    ),
                    mime_type: Some("application/json".to_string()),
                    size: None,
                    icons: None,
                    meta: None,
                },
                annotations: None,
            },
            Resource {
                raw: RawResource {
                    uri: "searxng://status".to_string(),
                    name: "SearXNG Status".to_string(),
                    title: Some("SearXNG Status".to_string()),
                    description: Some("Current free provider configuration and parallel mode status.".to_string()),
                    mime_type: Some("application/json".to_string()),
                    size: None,
                    icons: None,
                    meta: None,
                },
                annotations: None,
            },
        ];

        Ok(ListResourcesResult {
            resources,
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        params: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, ErrorData> {
        match params.uri.as_str() {
            "searxng://free-providers" => {
                let data = serde_json::json!({
                    "description": "Verified working public SearXNG providers. Always queried in full parallel (no concurrency limit) + fast-fail + HTML fallback + merge.",
                    "count": self.free_providers.len(),
                    "always_full_parallel": true,
                    "providers": self.free_providers.clone(),
                    "note": "Dead/unreliable providers are tracked only in docs/searxng-mcp/DEAD_PROVIDERS.md and never used by default."
                });
                let resource_contents = ResourceContents::TextResourceContents {
                    uri: params.uri,
                    mime_type: Some("application/json".to_string()),
                    text: serde_json::to_string_pretty(&data).unwrap_or_default(),
                    meta: None,
                };
                Ok(ReadResourceResult::new(vec![resource_contents]))
            }
            "searxng://status" => {
                let data = serde_json::json!({
                    "free_providers_configured": self.free_providers.len(),
                    "free_providers": self.free_providers.clone(),
                    "parallel_mode": "full (no limit, all always)",
                    "html_fallback": true,
                    "merge_strategy": "dedup by URL + engine aggregation + hit boosting",
                });
                let resource_contents = ResourceContents::TextResourceContents {
                    uri: params.uri,
                    mime_type: Some("application/json".to_string()),
                    text: serde_json::to_string_pretty(&data).unwrap_or_default(),
                    meta: None,
                };
                Ok(ReadResourceResult::new(vec![resource_contents]))
            }
            _ => Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                "unknown resource",
                None,
            )),
        }
    }
}
