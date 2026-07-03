use indoc::indoc;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, ErrorData, Implementation, InitializeResult, ServerCapabilities, ServerInfo,
    },
    tool, tool_handler, tool_router, ServerHandler,
};

use super::background_process::{
    GetBackgroundProcessOutputParams, ListBackgroundProcessesParams, StopBackgroundProcessParams,
};
use super::edit::{EditTools, FileEditParams, FileWriteParams};
use super::image::{ImageReadParams, ImageTool};
use super::shell::{ShellParams, ShellTool};
use super::tree::{TreeParams, TreeTool};

fn developer_instructions() -> &'static str {
    if cfg!(windows) {
        indoc! {"
            Use the developer extension to build software and operate a terminal.

            Make sure to use the tools *efficiently* - reading all the content you need in as few
            iterations as possible and then making the requested edits or running commands. You are
            responsible for managing your context window, and to minimize unnecessary turns which
            cost the user money.

            For editing software, prefer the flow of using tree to understand the codebase structure
            and file sizes. When you need to search, prefer findstr or Select-String (via shell).
            Then use type or Get-Content to gather the context you need, always reading before
            editing. Use write and edit to efficiently make changes. Test and verify as appropriate.
        "}
    } else {
        indoc! {"
            Use the developer extension to build software and operate a terminal.

            Make sure to use the tools *efficiently* - reading all the content you need in as few
            iterations as possible and then making the requested edits or running commands. You are
            responsible for managing your context window, and to minimize unnecessary turns which
            cost the user money.

            For editing software, prefer the flow of using tree to understand the codebase structure
            and file sizes. When you need to search, prefer rg which correctly respects gitignored
            content. Then use cat or sed to gather the context you need, always reading before editing.
            Use write and edit to efficiently make changes. Test and verify as appropriate.

            When running Python scripts or commands, always use `python3` instead of `python`.
        "}
    }
}

/// Developer MCP Server using the official RMCP SDK.
///
/// Exposes the same tools as goose's built-in `developer` platform extension
/// (write, edit, shell, tree, read_image) as a standalone stdio MCP server.
/// All file operations resolve relative paths against the process working
/// directory.
pub struct DeveloperServer {
    tool_router: ToolRouter<Self>,
    shell_tool: ShellTool,
    edit_tools: EditTools,
    tree_tool: TreeTool,
    image_tool: ImageTool,
}

#[tool_router(router = tool_router)]
impl DeveloperServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
            shell_tool: ShellTool::new(false).expect("failed to initialize shell tool"),
            edit_tools: EditTools::new(),
            tree_tool: TreeTool::new(),
            image_tool: ImageTool::new(),
        }
    }

    #[tool(
        name = "write",
        description = "Create a new file or overwrite an existing file. Creates parent directories if needed."
    )]
    pub async fn write(
        &self,
        params: Parameters<FileWriteParams>,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(self.edit_tools.file_write(params.0))
    }

    #[tool(
        name = "edit",
        description = "Edit a file by finding and replacing text. The before text must match exactly and uniquely. Use empty after text to delete."
    )]
    pub async fn edit(
        &self,
        params: Parameters<FileEditParams>,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(self.edit_tools.file_edit(params.0))
    }

    #[tool(
        name = "shell",
        description = "Execute a shell command in the current dir. Returns an object with stdout and stderr as separate fields. The output of each stream is limited to up to 2000 lines, and longer outputs will be saved to a temporary file."
    )]
    pub async fn shell(
        &self,
        params: Parameters<ShellParams>,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(self.shell_tool.shell(params.0).await)
    }

    #[tool(
        name = "list_background_processes",
        description = "List background processes started via shell(background: true), with their status (running/completed/failed/cancelled), PID, and elapsed time."
    )]
    pub async fn list_background_processes(
        &self,
        _params: Parameters<ListBackgroundProcessesParams>,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(self.shell_tool.list_background_processes().await)
    }

    #[tool(
        name = "get_background_process_output",
        description = "Get buffered output from a background process (default: last 50 lines, max 500). Use the process ID returned by shell(background: true) or list_background_processes."
    )]
    pub async fn get_background_process_output(
        &self,
        params: Parameters<GetBackgroundProcessOutputParams>,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(self
            .shell_tool
            .get_background_process_output(params.0)
            .await)
    }

    #[tool(
        name = "stop_background_process",
        description = "Stop a running background process, terminating it and its child processes."
    )]
    pub async fn stop_background_process(
        &self,
        params: Parameters<StopBackgroundProcessParams>,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(self.shell_tool.stop_background_process(params.0).await)
    }

    #[tool(
        name = "tree",
        description = "List a directory tree with line counts. Traversal respects .gitignore rules."
    )]
    pub async fn tree(&self, params: Parameters<TreeParams>) -> Result<CallToolResult, ErrorData> {
        Ok(self.tree_tool.tree(params.0))
    }

    #[tool(
        name = "read_image",
        description = "Read an image from a local file path or http(s) URL and return it as image content for the model to inspect. Supports png, jpeg, gif, and webp."
    )]
    pub async fn read_image(
        &self,
        params: Parameters<ImageReadParams>,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(self.image_tool.image_read_with_cwd(params.0, None).await)
    }
}

impl Default for DeveloperServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for DeveloperServer {
    fn get_info(&self) -> ServerInfo {
        InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "goose-developer",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(developer_instructions())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::handler::server::wrapper::Parameters;
    use std::fs;

    #[cfg(not(windows))]
    fn first_text(result: &CallToolResult) -> &str {
        use rmcp::model::RawContent;
        match &result.content[0].raw {
            RawContent::Text(text) => &text.text,
            _ => panic!("expected text content"),
        }
    }

    #[tokio::test]
    async fn write_then_edit_roundtrips() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("notes.txt");
        let server = DeveloperServer::new();

        let write = server
            .write(Parameters(FileWriteParams {
                path: path.to_string_lossy().to_string(),
                content: "first line".to_string(),
            }))
            .await
            .unwrap();
        assert_eq!(write.is_error, Some(false));
        assert_eq!(fs::read_to_string(&path).unwrap(), "first line");

        let edit = server
            .edit(Parameters(FileEditParams {
                path: path.to_string_lossy().to_string(),
                before: "first".to_string(),
                after: "updated".to_string(),
            }))
            .await
            .unwrap();
        assert_eq!(edit.is_error, Some(false));
        assert_eq!(fs::read_to_string(&path).unwrap(), "updated line");
    }

    #[tokio::test]
    async fn get_info_exposes_developer_instructions() {
        let server = DeveloperServer::new();
        let info = server.get_info();

        assert_eq!(info.server_info.name, "goose-developer");
        let instructions = info.instructions.unwrap();
        assert!(instructions.contains("Use the developer extension"));
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn shell_runs_in_process_cwd() {
        let server = DeveloperServer::new();
        let result = server
            .shell(Parameters(ShellParams {
                command: "echo hello".to_string(),
                timeout_secs: None,
                background: None,
            }))
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(false));
        assert!(first_text(&result).contains("hello"));
    }
}
