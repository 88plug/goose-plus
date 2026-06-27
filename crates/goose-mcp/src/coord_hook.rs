//! Injectable file-coordination hook.
//!
//! goose-mcp cannot depend on goose (that would be a dependency cycle), but the
//! NATS claim/lease coordinator lives in goose. So goose-mcp exposes this thin
//! hook: goose installs the real claim implementation at startup via
//! [`set_file_claim`], and the developer tools call [`claim_file`] before
//! mutating a file. When nothing is installed (e.g. the standalone
//! `goose mcp developer` server), it's a no-op and callers proceed uncoordinated.

use std::future::Future;
use std::pin::Pin;
use std::sync::OnceLock;

/// Opaque RAII guard held for the duration of a file mutation; dropping it
/// releases the underlying lease. (Concretely a `goose::nats::coord::Lease`,
/// boxed so goose-mcp needn't know the type.)
pub type Guard = Box<dyn std::any::Any + Send>;

/// Async closure that acquires a guard for a path.
pub type ClaimFuture = Pin<Box<dyn Future<Output = Guard> + Send>>;
pub type ClaimFn = Box<dyn Fn(String) -> ClaimFuture + Send + Sync>;

static HOOK: OnceLock<ClaimFn> = OnceLock::new();

/// Install the file coordinator. Called once by goose at startup; idempotent.
pub fn set_file_claim(f: ClaimFn) {
    let _ = HOOK.set(f);
}

/// Acquire a coordination guard for `path` before mutating it. Hold the returned
/// guard for the duration of the write (drop releases). Returns `None` when no
/// coordinator is installed — callers then proceed without coordination.
pub async fn claim_file(path: &str) -> Option<Guard> {
    let f = HOOK.get()?;
    Some(f(path.to_string()).await)
}

/// Synchronous bridge for the developer tools, which mutate files from sync
/// functions. Runs the async claim on the current Tokio runtime via
/// `block_in_place` (the tools execute inside the agent's multi-threaded
/// runtime). Returns `None` — proceed uncoordinated — when no coordinator is
/// installed (e.g. the standalone `goose mcp developer` server) or when called
/// outside a Tokio runtime. The claim is a fast no-op when coordination is
/// disabled, so this stays cheap on the common path.
pub fn claim_file_blocking(path: &str) -> Option<Guard> {
    let f = HOOK.get()?;
    let path = path.to_string();
    let handle = tokio::runtime::Handle::try_current().ok()?;
    Some(tokio::task::block_in_place(move || {
        handle.block_on(f(path))
    }))
}
