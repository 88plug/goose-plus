use rmcp::model::{CallToolResult, Content};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SearchParams {
    /// Search query (e.g., "rust tokio timeout", "react useEffect cleanup")
    pub query: String,
}

const SEARCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);
const SEARCH_LIMIT: usize = 5;

pub struct SearchTool {
    http_client: reqwest::Client,
}

impl SearchTool {
    pub fn new() -> Self {
        Self {
            http_client: reqwest::Client::builder()
                .user_agent(format!("goose-developer/{}", env!("CARGO_PKG_VERSION")))
                .connect_timeout(std::time::Duration::from_secs(5))
                .build()
                .expect("failed to build http client"),
        }
    }

    pub async fn search(&self, params: SearchParams) -> CallToolResult {
        let query = params.query.trim();

        if query.is_empty() {
            return CallToolResult::error(vec![
                Content::text("Search query cannot be empty").with_priority(0.0)
            ]);
        }

        let (sg, gh, rd) = tokio::join!(
            self.search_sourcegraph(query, SEARCH_TIMEOUT, SEARCH_LIMIT),
            self.search_github_issues(query, SEARCH_TIMEOUT, SEARCH_LIMIT),
            self.search_reddit(query, SEARCH_TIMEOUT, SEARCH_LIMIT)
        );

        let all_failed = sg.is_err() && gh.is_err() && rd.is_err();
        let sources = [
            (sg, SearchSource::Sourcegraph, "sourcegraph"),
            (gh, SearchSource::GitHubIssues, "github"),
            (rd, SearchSource::Reddit, "reddit"),
        ];

        let mut results = Vec::new();
        let mut active_sources = Vec::new();
        for (result, source, name) in sources {
            let items = result.unwrap_or_else(|e| {
                tracing::warn!("{}: {}", name, e);
                vec![]
            });
            if !items.is_empty() {
                active_sources.push(source);
            }
            results.extend(items);
        }

        let text = if all_failed {
            "Search unavailable.".to_string()
        } else if results.is_empty() {
            "No results found.".to_string()
        } else {
            format!(
                "{}{}",
                format_search_results(&results),
                build_fetch_hints(&active_sources)
            )
        };

        CallToolResult::success(vec![Content::text(text).with_priority(0.0)])
    }

    async fn search_sourcegraph(
        &self,
        query: &str,
        timeout: std::time::Duration,
        limit: usize,
    ) -> Result<Vec<SearchResult>, String> {
        let sg_query = format!(
            "{} -file:\\.md$ -file:\\.json$ -file:\\.yaml$ -file:\\.yml$ \
             -file:_test -file:test_ -file:/tests/ -file:__tests__ \
             -file:vendor/ -file:node_modules/ -file:third_party/ \
             fork:yes count:{}",
            query, limit
        );

        let url = reqwest::Url::parse_with_params(
            "https://sourcegraph.com/.api/search/stream",
            &[
                ("q", sg_query.as_str()),
                ("v", "V3"),
                ("t", "literal"),
                ("display", "100"),
            ],
        )
        .map_err(|e| e.to_string())?;

        let resp = self
            .http_client
            .get(url)
            .timeout(timeout)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }

        let text = resp.text().await.map_err(|e| e.to_string())?;

        let mut results = Vec::new();
        let mut in_matches = false;

        for line in text.lines() {
            if line == "event: matches" {
                in_matches = true;
                continue;
            }
            if line.starts_with("event:") {
                in_matches = false;
                continue;
            }
            if !in_matches || !line.starts_with("data:") {
                continue;
            }

            let json_str = line.strip_prefix("data:").unwrap_or(line).trim();
            let matches: Vec<serde_json::Value> = match serde_json::from_str(json_str) {
                Ok(m) => m,
                Err(_) => continue,
            };

            for m in matches.iter().take(limit.saturating_sub(results.len())) {
                let repo = m.get("repository").and_then(|r| r.as_str()).unwrap_or("");
                let path = m.get("path").and_then(|p| p.as_str()).unwrap_or("");
                if repo.is_empty() || path.is_empty() {
                    continue;
                }

                let repo_clean = repo.strip_prefix("github.com/").unwrap_or(repo);
                let stars = m.get("repoStars").and_then(|s| s.as_u64()).unwrap_or(0);
                let meta = if stars > 0 {
                    format!(" ({}★)", stars)
                } else {
                    String::new()
                };

                let snippet = m
                    .get("lineMatches")
                    .and_then(|lm| lm.as_array())
                    .and_then(|arr| {
                        arr.iter()
                            .filter_map(|lm| lm.get("line").and_then(|l| l.as_str()))
                            .next()
                            .map(|s| s.trim().to_string())
                    })
                    .unwrap_or_default();

                let raw_url = format!(
                    "https://raw.githubusercontent.com/{}/HEAD/{}",
                    repo_clean, path
                );

                results.push(SearchResult {
                    source: SearchSource::Sourcegraph,
                    title: format!("{}/{}{}", repo_clean, path, meta),
                    url: format!("https://github.com/{}/blob/HEAD/{}", repo_clean, path),
                    snippet: truncate(&snippet, 150),
                    fetch_url: Some(raw_url),
                });
            }

            if results.len() >= limit {
                break;
            }
        }
        Ok(results)
    }

    async fn search_github_issues(
        &self,
        query: &str,
        timeout: std::time::Duration,
        limit: usize,
    ) -> Result<Vec<SearchResult>, String> {
        use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};

        let gh_query = format!("{} is:issue", query);
        let encoded_query = utf8_percent_encode(&gh_query, NON_ALPHANUMERIC).to_string();
        let url = format!(
            "https://api.github.com/search/issues?q={}&per_page=20",
            encoded_query
        );

        let mut req = self
            .http_client
            .get(&url)
            .timeout(timeout)
            .header("Accept", "application/vnd.github+json");

        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        let resp = req.send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }

        let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        let mut results = Vec::new();

        if let Some(items) = json.get("items").and_then(|i| i.as_array()) {
            for item in items {
                if results.len() >= limit {
                    break;
                }

                let title = item.get("title").and_then(|t| t.as_str()).unwrap_or("");
                if title.is_empty() || is_bot_issue(title) {
                    continue;
                }

                let html_url = item.get("html_url").and_then(|u| u.as_str()).unwrap_or("");
                let state = item.get("state").and_then(|s| s.as_str()).unwrap_or("open");
                let comments = item.get("comments").and_then(|c| c.as_u64()).unwrap_or(0);
                let reactions = item
                    .pointer("/reactions/total_count")
                    .and_then(|c| c.as_u64())
                    .unwrap_or(0);
                let date = item
                    .get("created_at")
                    .and_then(|d| d.as_str())
                    .and_then(|s| s.get(..7))
                    .unwrap_or("");
                let repo = item
                    .get("repository_url")
                    .and_then(|u| u.as_str())
                    .and_then(|u| u.strip_prefix("https://api.github.com/repos/"))
                    .unwrap_or("");

                let mut meta = state.to_string();
                if reactions > 0 {
                    meta.push_str(&format!(", {}👍", reactions));
                }
                if comments > 0 {
                    meta.push_str(&format!(", {} comments", comments));
                }
                if !date.is_empty() {
                    meta.push_str(&format!(", {}", date));
                }
                if !repo.is_empty() {
                    meta.push_str(&format!(" [{}]", repo));
                }

                let body = item.get("body").and_then(|b| b.as_str()).unwrap_or("");

                let api_url = item.get("url").and_then(|u| u.as_str()).map(String::from);

                results.push(SearchResult {
                    source: SearchSource::GitHubIssues,
                    title: format!("{} ({})", title, meta),
                    url: html_url.to_string(),
                    snippet: truncate(body, 150),
                    fetch_url: api_url,
                });
            }
        }
        Ok(results)
    }

    async fn search_reddit(
        &self,
        query: &str,
        timeout: std::time::Duration,
        limit: usize,
    ) -> Result<Vec<SearchResult>, String> {
        use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};

        let subreddits = TECH_SUBREDDITS.join("+");
        let encoded_query = utf8_percent_encode(query, NON_ALPHANUMERIC).to_string();
        let url = format!(
            "https://www.reddit.com/r/{}/search.json?q={}&restrict_sr=1&limit={}&sort=relevance",
            subreddits, encoded_query, limit
        );

        let resp = self
            .http_client
            .get(&url)
            .timeout(timeout)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }

        let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        let mut results = Vec::new();

        if let Some(posts) = json.pointer("/data/children").and_then(|c| c.as_array()) {
            for post in posts.iter().take(limit) {
                let data = match post.get("data") {
                    Some(d) => d,
                    None => continue,
                };

                let title = data.get("title").and_then(|t| t.as_str()).unwrap_or("");
                if title.is_empty() {
                    continue;
                }

                let subreddit = data.get("subreddit").and_then(|s| s.as_str()).unwrap_or("");
                let permalink = data.get("permalink").and_then(|p| p.as_str()).unwrap_or("");
                let score = data.get("score").and_then(|s| s.as_i64()).unwrap_or(0);
                let num_comments = data
                    .get("num_comments")
                    .and_then(|n| n.as_i64())
                    .unwrap_or(0);
                let selftext = data.get("selftext").and_then(|s| s.as_str()).unwrap_or("");

                let meta = format!("r/{}, {}↑, {} comments", subreddit, score, num_comments);

                results.push(SearchResult {
                    source: SearchSource::Reddit,
                    title: format!("{} ({})", title, meta),
                    url: format!("https://www.reddit.com{}", permalink),
                    snippet: truncate(selftext, 150),
                    fetch_url: Some(format!("https://www.reddit.com{}.json", permalink)),
                });
            }
        }

        Ok(results)
    }
}

impl Default for SearchTool {
    fn default() -> Self {
        Self::new()
    }
}

const TECH_SUBREDDITS: &[&str] = &[
    "rust",
    "golang",
    "python",
    "javascript",
    "typescript",
    "programming",
    "coding",
    "learnprogramming",
    "webdev",
    "frontend",
    "backend",
    "devops",
    "kubernetes",
    "docker",
    "machinelearning",
    "datascience",
    "linux",
    "commandline",
    "vim",
    "neovim",
    "opensource",
    "selfhosted",
    "node",
    "reactjs",
    "django",
    "flask",
    "aws",
    "azure",
    "googlecloud",
    "databases",
    "sql",
    "mongodb",
    "redis",
    "elasticsearch",
    "netsec",
    "cybersecurity",
    "algorithms",
    "compsci",
    "cpp",
    "java",
    "csharp",
    "swift",
    "kotlin",
    "scala",
    "haskell",
    "elixir",
    "erlang",
    "clojure",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchSource {
    Sourcegraph,
    GitHubIssues,
    Reddit,
}

#[derive(Debug)]
struct SearchResult {
    source: SearchSource,
    title: String,
    url: String,
    snippet: String,
    fetch_url: Option<String>,
}

fn is_bot_issue(title: &str) -> bool {
    let t = title.to_lowercase();
    ["bump ", "dependabot", "renovate", "chore(deps)"]
        .iter()
        .any(|p| t.contains(p))
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    format!("{}...", s.chars().take(max).collect::<String>().trim_end())
}

fn format_search_results(results: &[SearchResult]) -> String {
    let mut lines = Vec::new();

    for r in results {
        let tag = match r.source {
            SearchSource::Sourcegraph => "code",
            SearchSource::GitHubIssues => "issue",
            SearchSource::Reddit => "reddit",
        };

        let fetch_hint = r
            .fetch_url
            .as_ref()
            .map(|u| format!("\n  fetch: {}", u))
            .unwrap_or_default();
        lines.push(format!(
            "[{}] {} - {}\n  {}{}",
            tag, r.title, r.url, r.snippet, fetch_hint
        ));
    }

    lines.join("\n\n")
}

fn build_fetch_hints(sources: &[SearchSource]) -> String {
    let mut hints = Vec::new();

    if sources.contains(&SearchSource::Sourcegraph) {
        hints.push("Code: `shell curl -sH 'Accept: text/plain' '<fetch_url>'`");
    }
    if sources.contains(&SearchSource::GitHubIssues) {
        hints.push("Issues: `shell curl -sH 'Accept: application/vnd.github+json' '<fetch_url>'` (use GITHUB_TOKEN if set)");
    }
    if sources.contains(&SearchSource::Reddit) {
        hints.push("Reddit: `shell curl -s '<fetch_url>'` (JSON format)");
    }

    if hints.is_empty() {
        return String::new();
    }

    format!("\n\n---\nTo fetch full content:\n{}", hints.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_bot_issue_matches_common_dependency_bump_titles() {
        assert!(is_bot_issue("Bump tokio from 1.0 to 1.1"));
        assert!(is_bot_issue("chore(deps): update dependabot config"));
        assert!(is_bot_issue("Update dependency foo (renovate)"));
    }

    #[test]
    fn is_bot_issue_ignores_normal_titles() {
        assert!(!is_bot_issue("Shell tool hangs on long-running commands"));
        assert!(!is_bot_issue("Support GOOSE_SHELL_ALLOWED_COMMANDS"));
    }

    #[test]
    fn truncate_leaves_short_strings_unchanged() {
        assert_eq!(truncate("hello world", 150), "hello world");
    }

    #[test]
    fn truncate_shortens_long_strings_with_ellipsis() {
        let long = "a".repeat(200);
        let truncated = truncate(&long, 150);
        assert_eq!(truncated.chars().count(), 153);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn truncate_counts_chars_not_bytes_for_multibyte_input() {
        let s = "★".repeat(200);
        let truncated = truncate(&s, 150);
        assert_eq!(truncated.chars().count(), 153);
    }

    #[test]
    fn format_search_results_includes_source_tag_and_fetch_hint() {
        let results = vec![SearchResult {
            source: SearchSource::Sourcegraph,
            title: "foo/bar.rs".to_string(),
            url: "https://github.com/foo/bar".to_string(),
            snippet: "fn main() {}".to_string(),
            fetch_url: Some("https://raw.githubusercontent.com/foo/bar/HEAD/bar.rs".to_string()),
        }];

        let text = format_search_results(&results);
        assert!(text.contains("[code]"));
        assert!(text.contains("foo/bar.rs"));
        assert!(text.contains("fetch: https://raw.githubusercontent.com"));
    }

    #[test]
    fn format_search_results_omits_fetch_hint_when_absent() {
        let results = vec![SearchResult {
            source: SearchSource::Reddit,
            title: "How do I fix this".to_string(),
            url: "https://www.reddit.com/r/rust/1".to_string(),
            snippet: "".to_string(),
            fetch_url: None,
        }];

        let text = format_search_results(&results);
        assert!(!text.contains("fetch:"));
    }

    #[test]
    fn build_fetch_hints_only_mentions_active_sources() {
        let hints = build_fetch_hints(&[SearchSource::GitHubIssues]);
        assert!(hints.contains("Issues:"));
        assert!(!hints.contains("Code:"));
        assert!(!hints.contains("Reddit:"));
    }

    #[test]
    fn build_fetch_hints_empty_when_no_sources_active() {
        assert_eq!(build_fetch_hints(&[]), "");
    }

    #[tokio::test]
    async fn search_rejects_empty_query() {
        let tool = SearchTool::new();
        let result = tool
            .search(SearchParams {
                query: "   ".to_string(),
            })
            .await;
        assert!(result.is_error.unwrap_or(false));
    }
}
