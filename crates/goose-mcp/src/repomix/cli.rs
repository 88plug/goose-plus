//! Subprocess plumbing for shelling out to the `repomix` CLI.
//!
//! Mirrors `computercontroller`'s peekaboo pattern exactly (see
//! `crate::peekaboo::{is_peekaboo_installed, auto_install_peekaboo}` and
//! `ComputerControllerServer::ensure_peekaboo`): cached existence check ->
//! attempt auto-install -> friendly, actionable error naming both the
//! manual-install command and the recommended community fork, never a raw
//! "command not found."
//!
//! Unlike peekaboo, `repomix` (a Node/npm CLI) is not platform-specific, so
//! this module is unconditionally compiled with no `#[cfg(target_os = ...)]`
//! gate.

use crate::subprocess::{configure_subprocess, SubprocessExt};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::process::Command;

pub struct RepomixInstall {
    checked: AtomicBool,
}

impl Default for RepomixInstall {
    fn default() -> Self {
        Self::new()
    }
}

impl RepomixInstall {
    pub fn new() -> Self {
        Self {
            checked: AtomicBool::new(which::which("repomix").is_ok()),
        }
    }

    /// Ensures `repomix` is resolvable on PATH, attempting an npm-based
    /// auto-install if it's missing. Returns a friendly, actionable error
    /// (never a raw exec failure) if it can't be made available.
    pub async fn ensure_installed(&self) -> Result<(), String> {
        if self.checked.load(Ordering::Relaxed) {
            return Ok(());
        }
        if which::which("repomix").is_ok() {
            self.checked.store(true, Ordering::Relaxed);
            return Ok(());
        }

        if which::which("npm").is_err() {
            return Err(install_instructions(
                "npm was not found on PATH, so repomix could not be auto-installed.",
            ));
        }

        tracing::info!("repomix not found, attempting auto-install via npm");
        let mut cmd = Command::new("npm");
        cmd.args(["install", "-g", "repomix"]);
        configure_subprocess(&mut cmd);
        let output = cmd
            .set_no_window()
            .output()
            .await
            .map_err(|e| install_instructions(&format!("Failed to run npm: {e}")))?;

        if output.status.success() && which::which("repomix").is_ok() {
            self.checked.store(true, Ordering::Relaxed);
            tracing::info!("repomix installed successfully via npm");
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        Err(install_instructions(&format!(
            "npm install -g repomix failed (exit {}): {}",
            output.status, detail
        )))
    }
}

fn install_instructions(reason: &str) -> String {
    format!(
        "{reason}\n\
         Install manually with: npm install -g repomix\n\
         For the community-improved fork, build and link it instead: \
         cd <repomix-plus checkout> && npm run build && npm link"
    )
}

/// Runs `repomix` with the given args, returning (stdout, stderr) on success
/// or a friendly error string on non-zero exit / spawn failure.
pub async fn run_repomix(args: &[String], cwd: Option<&Path>) -> Result<(String, String), String> {
    let mut cmd = Command::new("repomix");
    cmd.args(args);
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    configure_subprocess(&mut cmd);
    let output = cmd
        .set_no_window()
        .output()
        .await
        .map_err(|e| format!("Failed to run repomix: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        let detail = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        return Err(format!("repomix exited with {}: {}", output.status, detail));
    }

    Ok((stdout, stderr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_instructions_include_both_paths() {
        let msg = install_instructions("repomix missing");
        assert!(msg.contains("npm install -g repomix"));
        assert!(msg.contains("repomix-plus"));
        assert!(msg.contains("repomix missing"));
    }
}
