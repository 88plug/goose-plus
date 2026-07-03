//! Tracks shell commands started with `background: true` so a long-running
//! process (a dev server, a watcher) doesn't block the shell tool while it
//! runs. The registry owns each spawned child, buffers its interleaved
//! output, and exposes list/get-output/stop operations the agent can call
//! independently of the original `shell` invocation.

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use rmcp::model::{CallToolResult, Content};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use super::shell::{build_direct_command, build_shell_command, collect_tagged_lines, ShellMode};

/// Ring-buffer capacity for a single process's captured output.
const OUTPUT_BUFFER_LINES: usize = 1000;
const DEFAULT_OUTPUT_LINES: usize = 50;
const MAX_OUTPUT_LINES: usize = 500;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListBackgroundProcessesParams {}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetBackgroundProcessOutputParams {
    /// The process ID returned by `shell` (with `background: true`) or `list_background_processes`.
    pub process_id: String,
    /// Number of most recent output lines to return (default 50, max 500).
    #[serde(default)]
    pub lines: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StopBackgroundProcessParams {
    /// The process ID to stop.
    pub process_id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum BackgroundProcessStatus {
    Running,
    Completed { exit_code: Option<i32> },
    Failed { error: String },
    Cancelled,
}

impl BackgroundProcessStatus {
    fn display(&self) -> String {
        match self {
            BackgroundProcessStatus::Running => "running".to_string(),
            BackgroundProcessStatus::Completed { exit_code } => {
                format!("completed (exit code {})", exit_code.unwrap_or(-1))
            }
            BackgroundProcessStatus::Failed { error } => format!("failed: {error}"),
            BackgroundProcessStatus::Cancelled => "cancelled".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BackgroundProcessInfo {
    pub id: String,
    pub command: String,
    pub pid: Option<u32>,
    pub status: BackgroundProcessStatus,
    pub elapsed_secs: u64,
}

struct ProcessState {
    command: String,
    pid: Option<u32>,
    started_at: Instant,
    status: BackgroundProcessStatus,
    output: VecDeque<String>,
}

impl ProcessState {
    fn info(&self, id: &str) -> BackgroundProcessInfo {
        BackgroundProcessInfo {
            id: id.to_string(),
            command: self.command.clone(),
            pid: self.pid,
            status: self.status.clone(),
            elapsed_secs: self.started_at.elapsed().as_secs(),
        }
    }
}

#[derive(Clone, Default)]
pub struct BackgroundProcessRegistry {
    processes: Arc<RwLock<HashMap<String, Arc<RwLock<ProcessState>>>>>,
    next_id: Arc<AtomicU64>,
}

impl BackgroundProcessRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawn `command_line` detached, returning immediately with a process ID
    /// the caller can use to check status, fetch output, or stop it later.
    pub(super) async fn spawn(
        &self,
        command_line: &str,
        working_dir: Option<&Path>,
        login_path: Option<&str>,
        shell_mode: &ShellMode,
    ) -> Result<BackgroundProcessInfo, String> {
        let mut command = match shell_mode {
            ShellMode::Unrestricted => build_shell_command(command_line, working_dir, login_path),
            ShellMode::AllowList(allowed) => {
                build_direct_command(command_line, allowed, working_dir)?
            }
        };
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.stdin(Stdio::null());

        let mut child = command
            .spawn()
            .map_err(|error| format!("Failed to spawn shell command: {error}"))?;
        let pid = child.id();

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Failed to capture stdout".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "Failed to capture stderr".to_string())?;

        let id = format!("bg-{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let state = Arc::new(RwLock::new(ProcessState {
            command: command_line.to_string(),
            pid,
            started_at: Instant::now(),
            status: BackgroundProcessStatus::Running,
            output: VecDeque::new(),
        }));

        self.processes
            .write()
            .await
            .insert(id.clone(), state.clone());

        tokio::spawn(async move {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let collector = tokio::spawn(collect_tagged_lines(stdout, stderr, tx));

            while let Some((_is_stderr, line)) = rx.recv().await {
                let mut guard = state.write().await;
                if guard.output.len() >= OUTPUT_BUFFER_LINES {
                    guard.output.pop_front();
                }
                guard.output.push_back(line);
            }
            let _ = collector.await;

            let status = match child.wait().await {
                Ok(exit_status) if exit_status.success() => BackgroundProcessStatus::Completed {
                    exit_code: exit_status.code(),
                },
                Ok(exit_status) => BackgroundProcessStatus::Failed {
                    error: format!("process exited with code {:?}", exit_status.code()),
                },
                Err(error) => BackgroundProcessStatus::Failed {
                    error: error.to_string(),
                },
            };

            let mut guard = state.write().await;
            // A concurrent stop() already set a terminal status; don't clobber it.
            if guard.status == BackgroundProcessStatus::Running {
                guard.status = status;
            }
        });

        let guard = self.processes.read().await;
        let state = guard.get(&id).expect("just inserted").clone();
        drop(guard);
        let info = state.read().await.info(&id);
        Ok(info)
    }

    pub async fn list(&self) -> Vec<BackgroundProcessInfo> {
        let mut infos = Vec::new();
        for (id, state) in self.processes.read().await.iter() {
            infos.push(state.read().await.info(id));
        }
        infos.sort_by(|a, b| a.id.cmp(&b.id));
        infos
    }

    pub async fn get_output(
        &self,
        process_id: &str,
        lines: usize,
    ) -> Result<(BackgroundProcessInfo, Vec<String>), String> {
        let guard = self.processes.read().await;
        let state = guard
            .get(process_id)
            .ok_or_else(|| format!("No background process found with ID: {process_id}"))?;
        let state = state.read().await;
        let total = state.output.len();
        let start = total.saturating_sub(lines);
        let tail = state.output.iter().skip(start).cloned().collect();
        Ok((state.info(process_id), tail))
    }

    pub async fn stop(&self, process_id: &str) -> Result<BackgroundProcessInfo, String> {
        let guard = self.processes.read().await;
        let state = guard
            .get(process_id)
            .ok_or_else(|| format!("No background process found with ID: {process_id}"))?;
        let mut state = state.write().await;
        if state.status != BackgroundProcessStatus::Running {
            return Err(format!(
                "Process {process_id} is not running ({})",
                state.status.display()
            ));
        }
        if let Some(pid) = state.pid {
            terminate_process_group(pid);
        }
        state.status = BackgroundProcessStatus::Cancelled;
        Ok(state.info(process_id))
    }
}

#[cfg(unix)]
fn terminate_process_group(pid: u32) {
    // `configure_subprocess` puts the child in its own process group
    // (pgid == pid), so signaling the negated pid reaches any of its own
    // children too (e.g. a shell wrapper and the program it exec'd).
    unsafe {
        libc::kill(-(pid as libc::pid_t), libc::SIGTERM);
    }
}

#[cfg(windows)]
fn terminate_process_group(pid: u32) {
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .output();
}

pub fn format_process_list(processes: &[BackgroundProcessInfo]) -> CallToolResult {
    if processes.is_empty() {
        let text = "No background processes are currently tracked.";
        return CallToolResult::success(vec![Content::text(text).with_priority(0.0)]);
    }

    let mut text = String::from("Background processes:\n");
    for process in processes {
        text.push_str(&format!(
            "- {} [{}] pid={} elapsed={}s: {}\n",
            process.id,
            process.status.display(),
            process
                .pid
                .map(|p| p.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            process.elapsed_secs,
            process.command,
        ));
    }

    let mut result = CallToolResult::success(vec![Content::text(text).with_priority(0.0)]);
    result.structured_content = serde_json::to_value(processes).ok();
    result
}

pub fn format_process_started(info: &BackgroundProcessInfo) -> CallToolResult {
    let text = format!(
        "Started in background.\nProcess ID: {}\nCommand: {}\nPID: {}\n\n\
         The process is running independently — you can continue with other tasks.\n\
         Check status/output with get_background_process_output(\"{}\") or \
         list_background_processes; stop it with stop_background_process(\"{}\").",
        info.id,
        info.command,
        info.pid
            .map(|p| p.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        info.id,
        info.id,
    );
    let mut result = CallToolResult::success(vec![Content::text(text).with_priority(0.0)]);
    result.structured_content = serde_json::to_value(info).ok();
    result
}

pub fn format_process_output(info: &BackgroundProcessInfo, lines: &[String]) -> CallToolResult {
    let mut text = format!(
        "Process {} [{}] pid={} elapsed={}s: {}\n\n",
        info.id,
        info.status.display(),
        info.pid
            .map(|p| p.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        info.elapsed_secs,
        info.command,
    );
    if lines.is_empty() {
        text.push_str("(no output captured yet)");
    } else {
        for line in lines {
            text.push_str(line);
            text.push('\n');
        }
    }

    let mut result = CallToolResult::success(vec![Content::text(text).with_priority(0.0)]);
    result.structured_content = serde_json::to_value(info).ok();
    result
}

pub fn format_process_stopped(info: &BackgroundProcessInfo) -> CallToolResult {
    let text = format!("Stopped process {} ({})", info.id, info.command);
    let mut result = CallToolResult::success(vec![Content::text(text).with_priority(0.0)]);
    result.structured_content = serde_json::to_value(info).ok();
    result
}

pub fn error_result(message: String) -> CallToolResult {
    CallToolResult::error(vec![Content::text(message).with_priority(0.0)])
}

pub fn clamp_lines(requested: Option<usize>) -> usize {
    requested
        .unwrap_or(DEFAULT_OUTPUT_LINES)
        .min(MAX_OUTPUT_LINES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn spawn_tracks_completion_status_and_output() {
        let registry = BackgroundProcessRegistry::new();
        let info = registry
            .spawn(
                "echo hello-background",
                None,
                None,
                &ShellMode::Unrestricted,
            )
            .await
            .expect("spawn should succeed");
        assert_eq!(info.status, BackgroundProcessStatus::Running);

        // Poll until the monitor task observes completion.
        let mut completed = None;
        for _ in 0..100 {
            let (current, output) = registry.get_output(&info.id, 10).await.unwrap();
            if current.status != BackgroundProcessStatus::Running {
                completed = Some((current, output));
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        let (final_info, output) = completed.expect("process should complete");
        assert_eq!(
            final_info.status,
            BackgroundProcessStatus::Completed { exit_code: Some(0) }
        );
        assert!(output.iter().any(|line| line.contains("hello-background")));
    }

    #[tokio::test]
    async fn list_reports_all_spawned_processes() {
        let registry = BackgroundProcessRegistry::new();
        let a = registry
            .spawn("echo a", None, None, &ShellMode::Unrestricted)
            .await
            .unwrap();
        let b = registry
            .spawn("echo b", None, None, &ShellMode::Unrestricted)
            .await
            .unwrap();

        let listed = registry.list().await;
        let ids: Vec<&str> = listed.iter().map(|p| p.id.as_str()).collect();
        assert!(ids.contains(&a.id.as_str()));
        assert!(ids.contains(&b.id.as_str()));
    }

    #[tokio::test]
    async fn stop_terminates_a_long_running_process() {
        let registry = BackgroundProcessRegistry::new();
        let info = registry
            .spawn("sleep 30", None, None, &ShellMode::Unrestricted)
            .await
            .unwrap();

        let stopped = registry.stop(&info.id).await.expect("stop should succeed");
        assert_eq!(stopped.status, BackgroundProcessStatus::Cancelled);

        // Stopping again should fail: no longer running.
        let err = registry.stop(&info.id).await.unwrap_err();
        assert!(err.contains("not running"));
    }

    #[tokio::test]
    async fn get_output_reports_missing_process() {
        let registry = BackgroundProcessRegistry::new();
        let err = registry.get_output("bg-999", 10).await.unwrap_err();
        assert!(err.contains("No background process found"));
    }

    #[test]
    fn clamp_lines_applies_default_and_max() {
        assert_eq!(clamp_lines(None), DEFAULT_OUTPUT_LINES);
        assert_eq!(clamp_lines(Some(10_000)), MAX_OUTPUT_LINES);
        assert_eq!(clamp_lines(Some(5)), 5);
    }
}
