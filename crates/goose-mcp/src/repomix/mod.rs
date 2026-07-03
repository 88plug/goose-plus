/*!
 * Repomix MCP extension for goose.
 *
 * Repomix packages a codebase (local directory or remote GitHub repo) into a
 * single consolidated file for AI analysis — directory tree, file contents,
 * and metrics in one document, with optional Tree-sitter compression and
 * multiple output formats.
 *
 * Embedded natively the same way `searxng` is: a built-in extension the
 * agent can load in-process (see `crate::BUILTIN_EXTENSIONS`), not just an
 * external MCP server the user has to configure by hand. Unlike searxng
 * (which reimplements its provider-fetch/merge logic natively), repomix
 * already ships its own MCP server backed by a real packing engine
 * (fast-glob matching, Tree-sitter compression across a dozen+ languages,
 * git cloning) that isn't worth reimplementing from scratch. So this
 * extension is a thin split:
 *   - the read-only, file/registry-only tools (`read_repomix_output`,
 *     `grep_repomix_output`, `attach_packed_output`, the `file_system_*`
 *     tools) are pure Rust — simpler and faster than shelling out
 *   - the tools that depend on repomix's packing engine (`pack_codebase`,
 *     `pack_remote_repository`, `generate_skill`) shell out to the real
 *     `repomix` CLI on PATH, auto-installed via npm if missing (see
 *     `cli::RepomixInstall`, modeled on `computercontroller`'s peekaboo
 *     auto-install pattern)
 */

mod cli;
mod metrics;
mod security;

use ignore::WalkBuilder;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, Content, ErrorCode, ErrorData, Implementation, InitializeResult,
        ServerCapabilities, ServerInfo,
    },
    schemars::JsonSchema,
    tool, tool_handler, tool_router, ServerHandler,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MAX_REGISTRY_SIZE: usize = 100;
const DEFAULT_OUTPUT_NAMES: &[&str] = &[
    "repomix-output.xml",
    "repomix-output.md",
    "repomix-output.json",
    "repomix-output.txt",
];

fn output_file_name(style: &str) -> &'static str {
    match style {
        "markdown" => "repomix-output.md",
        "json" => "repomix-output.json",
        "plain" => "repomix-output.txt",
        _ => "repomix-output.xml",
    }
}

/// Insertion-order-capped registry mapping opaque output IDs to file paths,
/// mirroring upstream repomix's own in-process `outputFileRegistry`
/// (`src/mcp/tools/mcpToolRuntime.ts`) so behavior — including the eviction
/// cap — matches exactly.
#[derive(Default)]
struct OutputRegistry {
    paths: HashMap<String, PathBuf>,
    order: VecDeque<String>,
}

impl OutputRegistry {
    fn register(&mut self, path: PathBuf) -> String {
        if self.paths.len() >= MAX_REGISTRY_SIZE {
            if let Some(oldest) = self.order.pop_front() {
                self.paths.remove(&oldest);
            }
        }
        let id: String = uuid::Uuid::new_v4()
            .simple()
            .to_string()
            .chars()
            .take(16)
            .collect();
        self.paths.insert(id.clone(), path);
        self.order.push_back(id.clone());
        id
    }

    fn get(&self, id: &str) -> Option<PathBuf> {
        self.paths.get(id).cloned()
    }
}

fn err(message: impl Into<String>) -> ErrorData {
    ErrorData::new(ErrorCode::INTERNAL_ERROR, message.into(), None)
}

fn invalid_params(message: impl Into<String>) -> ErrorData {
    ErrorData::new(ErrorCode::INVALID_PARAMS, message.into(), None)
}

fn text_result(text: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![Content::text(text.into())])
}

fn resolve_output_path(
    registry: &Mutex<OutputRegistry>,
    output_id: &str,
) -> Result<PathBuf, ErrorData> {
    registry
        .lock()
        .unwrap()
        .get(output_id)
        .ok_or_else(|| invalid_params(format!("Unknown outputId: {output_id}")))
}

async fn read_registered_output(
    registry: &Mutex<OutputRegistry>,
    output_id: &str,
) -> Result<String, ErrorData> {
    let path = resolve_output_path(registry, output_id)?;
    tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| err(format!("Failed to read {}: {e}", path.display())))
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PackCodebaseParams {
    /// Directory to pack (absolute path)
    pub directory: String,
    /// Enable Tree-sitter compression to extract essential code signatures
    /// while removing implementation details (default: false)
    #[serde(default)]
    pub compress: bool,
    /// Comma-separated fast-glob include patterns (e.g. "**/*.{js,ts}")
    pub include_patterns: Option<String>,
    /// Comma-separated fast-glob ignore patterns, supplementing .gitignore
    pub ignore_patterns: Option<String>,
    /// Number of largest files to list in the metrics summary (default: 10)
    #[serde(default = "default_top_files_length")]
    pub top_files_length: u32,
    /// Output format: xml (default), markdown, json, or plain
    #[serde(default = "default_style")]
    pub style: String,
    /// Include byte/line offsets for each file in the directory structure
    #[serde(default)]
    pub show_file_offsets: bool,
}

fn default_top_files_length() -> u32 {
    10
}
fn default_style() -> String {
    "xml".to_string()
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PackRemoteRepositoryParams {
    /// GitHub repository URL or user/repo shorthand
    pub remote: String,
    /// Branch, tag, or commit to pack (defaults to the repo's default branch)
    pub branch: Option<String>,
    #[serde(default)]
    pub compress: bool,
    pub include_patterns: Option<String>,
    pub ignore_patterns: Option<String>,
    #[serde(default = "default_top_files_length")]
    pub top_files_length: u32,
    #[serde(default = "default_style")]
    pub style: String,
    #[serde(default)]
    pub show_file_offsets: bool,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct AttachPackedOutputParams {
    /// Directory containing a repomix output file, or a direct path to one
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadRepomixOutputParams {
    pub output_id: String,
    /// 1-based starting line (inclusive). Omit to read from the beginning.
    pub start_line: Option<u32>,
    /// 1-based ending line (inclusive). Omit to read to the end.
    pub end_line: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GrepRepomixOutputParams {
    pub output_id: String,
    /// Regular expression pattern (Rust regex syntax)
    pub pattern: String,
    #[serde(default)]
    pub ignore_case: bool,
    /// Context lines shown before and after each match
    #[serde(default)]
    pub context_lines: u32,
    /// Context lines before each match (overrides context_lines)
    pub before_lines: Option<u32>,
    /// Context lines after each match (overrides context_lines)
    pub after_lines: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FileSystemReadDirectoryParams {
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FileSystemReadDirectoryWithSizesParams {
    pub path: String,
    /// Sort entries by "name" (default) or "size"
    #[serde(default = "default_sort_by")]
    pub sort_by: String,
}

fn default_sort_by() -> String {
    "name".to_string()
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FileSystemReadFileParams {
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GenerateSkillParams {
    pub directory: String,
    /// Skill name (kebab-case). Auto-generated from the directory name if omitted.
    pub skill_name: Option<String>,
    #[serde(default)]
    pub compress: bool,
    pub include_patterns: Option<String>,
    pub ignore_patterns: Option<String>,
}

/// Bundles the shared pack-invocation parameters so `build_pack_args`/`run_pack`
/// don't need a long positional-argument list.
struct PackRequest<'a> {
    directory: Option<&'a str>,
    remote: Option<&'a str>,
    branch: Option<&'a str>,
    compress: bool,
    include_patterns: Option<&'a str>,
    ignore_patterns: Option<&'a str>,
    top_files_length: u32,
    style: &'a str,
    show_file_offsets: bool,
}

#[derive(Clone)]
pub struct RepomixServer {
    tool_router: ToolRouter<Self>,
    registry: std::sync::Arc<Mutex<OutputRegistry>>,
    install: std::sync::Arc<cli::RepomixInstall>,
}

impl Default for RepomixServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router(router = tool_router)]
impl RepomixServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
            registry: std::sync::Arc::new(Mutex::new(OutputRegistry::default())),
            install: std::sync::Arc::new(cli::RepomixInstall::new()),
        }
    }

    fn build_pack_args(req: &PackRequest, output_path: &Path) -> Vec<String> {
        let mut args = vec![
            "--style".to_string(),
            req.style.to_string(),
            "--output".to_string(),
            output_path.display().to_string(),
            "--top-files-len".to_string(),
            req.top_files_length.to_string(),
            "--quiet".to_string(),
        ];
        if req.compress {
            args.push("--compress".to_string());
        }
        if req.show_file_offsets {
            args.push("--show-file-offsets".to_string());
        }
        if let Some(patterns) = req.include_patterns {
            args.push("--include".to_string());
            args.push(patterns.to_string());
        }
        if let Some(patterns) = req.ignore_patterns {
            args.push("--ignore".to_string());
            args.push(patterns.to_string());
        }
        if let Some(remote) = req.remote {
            args.push("--remote".to_string());
            args.push(remote.to_string());
            if let Some(branch) = req.branch {
                args.push("--remote-branch".to_string());
                args.push(branch.to_string());
            }
        }
        if let Some(dir) = req.directory {
            args.push(dir.to_string());
        }
        args
    }

    async fn run_pack(&self, req: PackRequest<'_>) -> Result<CallToolResult, ErrorData> {
        self.install.ensure_installed().await.map_err(err)?;

        let workspace = std::env::temp_dir().join(format!(
            "goose-repomix-mcp-{}",
            uuid::Uuid::new_v4().simple()
        ));
        tokio::fs::create_dir_all(&workspace)
            .await
            .map_err(|e| err(format!("Failed to create workspace: {e}")))?;

        let output_path = workspace.join(output_file_name(req.style));
        let args = Self::build_pack_args(&req, &output_path);

        // --quiet suppresses the summary lines metrics::extract_metrics reads,
        // so drop it just for this invocation to get the real totals.
        let args: Vec<String> = args.into_iter().filter(|a| a != "--quiet").collect();
        let (stdout, _stderr) = cli::run_repomix(&args, None).await.map_err(err)?;

        let content = tokio::fs::read_to_string(&output_path)
            .await
            .map_err(|e| err(format!("Failed to read packed output: {e}")))?;
        let m = metrics::extract_metrics(&stdout, &content);

        let output_id = self.registry.lock().unwrap().register(output_path.clone());

        let mut summary = vec![
            "🎉 Successfully packed codebase!".to_string(),
            String::new(),
        ];
        if let Some(d) = req.directory {
            summary.push(format!("Directory: {d}"));
        }
        if let Some(r) = req.remote {
            summary.push(format!("Repository: {r}"));
        }
        summary.push(format!("Output file: {}", output_path.display()));
        summary.push(format!("Output ID: {output_id}"));
        summary.push(format!("Total files: {}", m.total_files));
        summary.push(format!("Total tokens: {}", m.total_tokens));
        summary.push(format!("Total characters: {}", m.total_characters));
        summary.push(String::new());
        summary.push(format!(
            "Use `read_repomix_output` with outputId \"{output_id}\" to read the packed content, \
             or `grep_repomix_output` to search within it."
        ));

        Ok(text_result(summary.join("\n")))
    }

    #[tool(
        name = "pack_codebase",
        description = "Package a local code directory into a consolidated file for AI analysis. Supports Tree-sitter compression and xml/markdown/json/plain output styles. Shells out to the repomix CLI."
    )]
    async fn pack_codebase(
        &self,
        params: Parameters<PackCodebaseParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        self.run_pack(PackRequest {
            directory: Some(&p.directory),
            remote: None,
            branch: None,
            compress: p.compress,
            include_patterns: p.include_patterns.as_deref(),
            ignore_patterns: p.ignore_patterns.as_deref(),
            top_files_length: p.top_files_length,
            style: &p.style,
            show_file_offsets: p.show_file_offsets,
        })
        .await
    }

    #[tool(
        name = "pack_remote_repository",
        description = "Clone and pack a remote GitHub repository into a consolidated file for AI analysis. Shells out to the repomix CLI."
    )]
    async fn pack_remote_repository(
        &self,
        params: Parameters<PackRemoteRepositoryParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        self.run_pack(PackRequest {
            directory: None,
            remote: Some(&p.remote),
            branch: p.branch.as_deref(),
            compress: p.compress,
            include_patterns: p.include_patterns.as_deref(),
            ignore_patterns: p.ignore_patterns.as_deref(),
            top_files_length: p.top_files_length,
            style: &p.style,
            show_file_offsets: p.show_file_offsets,
        })
        .await
    }

    #[tool(
        name = "attach_packed_output",
        description = "Attach an existing repomix output file (or a directory containing one) so it can be read/grepped by outputId without re-packing."
    )]
    async fn attach_packed_output(
        &self,
        params: Parameters<AttachPackedOutputParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        let candidate = PathBuf::from(&p.path);

        let resolved = if candidate.is_dir() {
            DEFAULT_OUTPUT_NAMES
                .iter()
                .map(|name| candidate.join(name))
                .find(|path| path.is_file())
                .ok_or_else(|| {
                    invalid_params(format!(
                        "No repomix output file found in directory: {}",
                        candidate.display()
                    ))
                })?
        } else if candidate.is_file() {
            candidate
        } else {
            return Err(invalid_params(format!(
                "Path does not exist: {}",
                candidate.display()
            )));
        };

        let output_id = self.registry.lock().unwrap().register(resolved.clone());
        Ok(text_result(format!(
            "Attached {}\nOutput ID: {output_id}",
            resolved.display()
        )))
    }

    #[tool(
        name = "read_repomix_output",
        description = "Read the contents of a packed repomix output by outputId, optionally limited to a line range."
    )]
    async fn read_repomix_output(
        &self,
        params: Parameters<ReadRepomixOutputParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        let content = read_registered_output(&self.registry, &p.output_id).await?;

        if p.start_line.is_none() && p.end_line.is_none() {
            return Ok(text_result(content));
        }

        let start = p.start_line.unwrap_or(1).max(1) as usize;
        let end = p.end_line.map(|n| n as usize);
        let selected: Vec<&str> = content
            .lines()
            .enumerate()
            .filter(|(i, _)| {
                let line_no = i + 1;
                line_no >= start && end.is_none_or(|end| line_no <= end)
            })
            .map(|(_, line)| line)
            .collect();
        Ok(text_result(selected.join("\n")))
    }

    #[tool(
        name = "grep_repomix_output",
        description = "Search a packed repomix output for a regex pattern, with optional context lines and case-insensitive matching."
    )]
    async fn grep_repomix_output(
        &self,
        params: Parameters<GrepRepomixOutputParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        let content = read_registered_output(&self.registry, &p.output_id).await?;

        let re = regex::RegexBuilder::new(&p.pattern)
            .case_insensitive(p.ignore_case)
            .build()
            .map_err(|e| invalid_params(format!("Invalid regex pattern: {e}")))?;

        let before = p.before_lines.unwrap_or(p.context_lines) as usize;
        let after = p.after_lines.unwrap_or(p.context_lines) as usize;

        let lines: Vec<&str> = content.lines().collect();
        let mut matched_indices: Vec<usize> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if re.is_match(line) {
                matched_indices.push(i);
            }
        }

        if matched_indices.is_empty() {
            return Ok(text_result(format!(
                "No matches for pattern: {}",
                p.pattern
            )));
        }

        let mut output = Vec::new();
        let mut last_printed: Option<usize> = None;
        for &idx in &matched_indices {
            let start = idx.saturating_sub(before);
            let end = (idx + after).min(lines.len().saturating_sub(1));
            if let Some(last) = last_printed {
                if start > last + 1 {
                    output.push("--".to_string());
                }
            }
            for (line_idx, line) in lines.iter().enumerate().take(end + 1).skip(start) {
                if last_printed.is_some_and(|last| line_idx <= last) {
                    continue;
                }
                output.push(format!("{}: {}", line_idx + 1, line));
                last_printed = Some(line_idx);
            }
        }

        Ok(text_result(output.join("\n")))
    }

    #[tool(
        name = "file_system_read_directory",
        description = "List the immediate contents of a directory, distinguishing files and subdirectories."
    )]
    async fn file_system_read_directory(
        &self,
        params: Parameters<FileSystemReadDirectoryParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        let mut entries = read_dir_entries(&p.path).await?;
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        let lines: Vec<String> = entries
            .iter()
            .map(|e| format!("[{}] {}", if e.is_dir { "DIR" } else { "FILE" }, e.name))
            .collect();
        Ok(text_result(lines.join("\n")))
    }

    #[tool(
        name = "file_system_read_directory_with_sizes",
        description = "List the immediate contents of a directory with file sizes, sortable by name or size."
    )]
    async fn file_system_read_directory_with_sizes(
        &self,
        params: Parameters<FileSystemReadDirectoryWithSizesParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        let mut entries = read_dir_entries(&p.path).await?;
        if p.sort_by == "size" {
            entries.sort_by(|a, b| b.size.cmp(&a.size));
        } else {
            entries.sort_by(|a, b| a.name.cmp(&b.name));
        }

        let lines: Vec<String> = entries
            .iter()
            .map(|e| {
                format!(
                    "[{}] {} ({} bytes)",
                    if e.is_dir { "DIR" } else { "FILE" },
                    e.name,
                    e.size
                )
            })
            .collect();
        Ok(text_result(lines.join("\n")))
    }

    #[tool(
        name = "file_system_read_file",
        description = "Read a single file's contents. Refuses to serve files that appear to contain high-entropy secrets (API keys, tokens)."
    )]
    async fn file_system_read_file(
        &self,
        params: Parameters<FileSystemReadFileParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        let content = tokio::fs::read_to_string(&p.path)
            .await
            .map_err(|e| err(format!("Failed to read {}: {e}", p.path)))?;

        if let Some(preview) = security::detect_secret(&content) {
            return Err(invalid_params(format!(
                "Refusing to read {}: file appears to contain sensitive data ({preview})",
                p.path
            )));
        }

        Ok(text_result(content))
    }

    #[tool(
        name = "generate_skill",
        description = "Generate a Claude Agent Skill (SKILL.md + reference docs) from a local code directory. Shells out to the repomix CLI."
    )]
    async fn generate_skill(
        &self,
        params: Parameters<GenerateSkillParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        self.install.ensure_installed().await.map_err(err)?;

        let mut args = vec![
            "--skill-output".to_string(),
            ".".to_string(),
            "-f".to_string(),
        ];
        if p.compress {
            args.push("--compress".to_string());
        }
        if let Some(patterns) = &p.include_patterns {
            args.push("--include".to_string());
            args.push(patterns.clone());
        }
        if let Some(patterns) = &p.ignore_patterns {
            args.push("--ignore".to_string());
            args.push(patterns.clone());
        }
        if let Some(name) = &p.skill_name {
            args.push("--skill-name".to_string());
            args.push(name.clone());
        }
        args.push(p.directory.clone());

        let (stdout, _stderr) = cli::run_repomix(&args, None).await.map_err(err)?;
        Ok(text_result(if stdout.trim().is_empty() {
            format!("Skill generated from {}", p.directory)
        } else {
            stdout
        }))
    }
}

struct DirEntryInfo {
    name: String,
    is_dir: bool,
    size: u64,
}

async fn read_dir_entries(path: &str) -> Result<Vec<DirEntryInfo>, ErrorData> {
    let root = PathBuf::from(path);
    if !root.is_dir() {
        return Err(invalid_params(format!("Not a directory: {path}")));
    }

    let root_owned = root.clone();
    tokio::task::spawn_blocking(move || {
        let mut entries = Vec::new();
        for result in WalkBuilder::new(&root_owned)
            .max_depth(Some(1))
            .hidden(false)
            .git_ignore(false)
            .build()
        {
            let entry = match result {
                Ok(e) => e,
                Err(_) => continue,
            };
            if entry.path() == root_owned {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let metadata = entry.metadata().ok();
            let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
            let size = metadata.map(|m| m.len()).unwrap_or(0);
            entries.push(DirEntryInfo { name, is_dir, size });
        }
        entries
    })
    .await
    .map_err(|e| err(format!("Failed to list directory: {e}")))
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for RepomixServer {
    fn get_info(&self) -> ServerInfo {
        InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("repomix", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Repomix packs a local directory or remote GitHub repository into a single \
                 consolidated file for AI analysis (directory tree, file contents, metrics). \
                 Use pack_codebase/pack_remote_repository to produce an outputId, then \
                 read_repomix_output or grep_repomix_output to read it incrementally instead of \
                 loading the whole pack into context at once.",
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_file_name_maps_all_styles() {
        assert_eq!(output_file_name("xml"), "repomix-output.xml");
        assert_eq!(output_file_name("markdown"), "repomix-output.md");
        assert_eq!(output_file_name("json"), "repomix-output.json");
        assert_eq!(output_file_name("plain"), "repomix-output.txt");
        assert_eq!(output_file_name("unknown"), "repomix-output.xml");
    }

    #[test]
    fn registry_registers_and_resolves() {
        let mut registry = OutputRegistry::default();
        let id = registry.register(PathBuf::from("/tmp/example.xml"));
        assert_eq!(registry.get(&id), Some(PathBuf::from("/tmp/example.xml")));
        assert_eq!(registry.get("does-not-exist"), None);
    }

    #[test]
    fn registry_evicts_oldest_when_full() {
        let mut registry = OutputRegistry::default();
        let mut ids = Vec::new();
        for i in 0..MAX_REGISTRY_SIZE {
            ids.push(registry.register(PathBuf::from(format!("/tmp/{i}.xml"))));
        }
        assert!(registry.get(&ids[0]).is_some());

        let overflow_id = registry.register(PathBuf::from("/tmp/overflow.xml"));
        assert!(
            registry.get(&ids[0]).is_none(),
            "oldest entry should be evicted"
        );
        assert!(registry.get(&overflow_id).is_some());
        assert_eq!(registry.paths.len(), MAX_REGISTRY_SIZE);
    }

    #[test]
    fn build_pack_args_includes_directory_and_style() {
        let req = PackRequest {
            directory: Some("/some/dir"),
            remote: None,
            branch: None,
            compress: true,
            include_patterns: Some("**/*.rs"),
            ignore_patterns: Some("target/**"),
            top_files_length: 5,
            style: "markdown",
            show_file_offsets: true,
        };
        let args = RepomixServer::build_pack_args(&req, Path::new("/tmp/out.md"));
        assert!(args.contains(&"--compress".to_string()));
        assert!(args.contains(&"--show-file-offsets".to_string()));
        assert!(args.contains(&"markdown".to_string()));
        assert!(args.contains(&"/some/dir".to_string()));
        assert!(args.contains(&"**/*.rs".to_string()));
        assert!(args.contains(&"target/**".to_string()));
    }

    #[test]
    fn build_pack_args_includes_remote_and_branch() {
        let req = PackRequest {
            directory: None,
            remote: Some("owner/repo"),
            branch: Some("main"),
            compress: false,
            include_patterns: None,
            ignore_patterns: None,
            top_files_length: 10,
            style: "xml",
            show_file_offsets: false,
        };
        let args = RepomixServer::build_pack_args(&req, Path::new("/tmp/out.xml"));
        assert!(args.contains(&"--remote".to_string()));
        assert!(args.contains(&"owner/repo".to_string()));
        assert!(args.contains(&"--remote-branch".to_string()));
        assert!(args.contains(&"main".to_string()));
    }

    #[tokio::test]
    async fn get_info_reports_server_name() {
        let server = RepomixServer::new();
        let info = server.get_info();
        assert_eq!(info.server_info.name, "repomix");
        assert!(info.instructions.unwrap().contains("outputId"));
    }

    // Exercises the real subprocess round-trip against an actually-installed
    // `repomix` binary: pack -> read -> grep. Gated behind #[ignore] since CI
    // won't have repomix on PATH; run manually with `-- --ignored` on a
    // machine that does to catch real integration regressions (arg wiring,
    // stdout summary-format drift, output-file parsing).
    #[tokio::test]
    #[ignore]
    async fn live_pack_read_grep_round_trip() {
        let server = RepomixServer::new();
        let target_dir = env!("CARGO_MANIFEST_DIR").to_string() + "/src/repomix";

        let pack_result = server
            .pack_codebase(Parameters(PackCodebaseParams {
                directory: target_dir,
                compress: false,
                include_patterns: Some("*.rs".to_string()),
                ignore_patterns: None,
                top_files_length: 5,
                style: "xml".to_string(),
                show_file_offsets: false,
            }))
            .await
            .expect("pack_codebase should succeed with a real repomix install");

        let pack_text = pack_result.content[0].as_text().unwrap().text.clone();
        assert!(pack_text.contains("Output ID:"));
        let output_id = pack_text
            .lines()
            .find_map(|l| l.strip_prefix("Output ID: "))
            .expect("summary should contain an Output ID line")
            .trim()
            .to_string();

        let read_result = server
            .read_repomix_output(Parameters(ReadRepomixOutputParams {
                output_id: output_id.clone(),
                start_line: None,
                end_line: None,
            }))
            .await
            .expect("read_repomix_output should succeed for a just-packed id");
        let read_text = read_result.content[0].as_text().unwrap().text.clone();
        assert!(read_text.contains("mod.rs"));

        let grep_result = server
            .grep_repomix_output(Parameters(GrepRepomixOutputParams {
                output_id,
                pattern: "RepomixServer".to_string(),
                ignore_case: false,
                context_lines: 0,
                before_lines: None,
                after_lines: None,
            }))
            .await
            .expect("grep_repomix_output should succeed");
        let grep_text = grep_result.content[0].as_text().unwrap().text.clone();
        assert!(grep_text.contains("RepomixServer"));
    }
}
