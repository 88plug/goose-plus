use etcetera::AppStrategyArgs;
use once_cell::sync::Lazy;
use rmcp::{ServerHandler, ServiceExt};
use std::collections::HashMap;

// NOTE: "Block" is kept here for backwards compatibility with existing
// user config/data directories. Changing this would orphan existing installations.
pub static APP_STRATEGY: Lazy<AppStrategyArgs> = Lazy::new(|| AppStrategyArgs {
    top_level_domain: "Block".to_string(),
    author: "Block".to_string(),
    app_name: "goose-plus".to_string(),
});

pub mod autovisualiser;
pub mod computercontroller;
pub mod coord_hook;
pub mod developer;
pub mod mcp_server_runner;
mod memory;
#[cfg(target_os = "macos")]
pub mod peekaboo;
pub mod repomix;
pub mod searxng;
pub mod subprocess;
pub mod tutorial;
#[cfg(windows)]
pub mod windows_job;

pub use autovisualiser::AutoVisualiserRouter;
pub use computercontroller::ComputerControllerServer;
pub use developer::DeveloperServer;
pub use memory::MemoryServer;
pub use repomix::RepomixServer;
pub use searxng::{SearchUpdate, SearxngServer};
pub use tutorial::TutorialServer;

/// Type definition for a function that spawns and serves a builtin extension server
pub type SpawnServerFn = fn(tokio::io::DuplexStream, tokio::io::DuplexStream);

fn spawn_and_serve<S>(
    name: &'static str,
    server: S,
    transport: (tokio::io::DuplexStream, tokio::io::DuplexStream),
) where
    S: ServerHandler + Send + 'static,
{
    tokio::spawn(async move {
        match server.serve(transport).await {
            Ok(running) => {
                let _ = running.waiting().await;
            }
            Err(e) => tracing::error!(builtin = name, error = %e, "server error"),
        }
    });
}

macro_rules! builtin {
    ($name:ident, $server_ty:ty) => {{
        fn spawn(r: tokio::io::DuplexStream, w: tokio::io::DuplexStream) {
            spawn_and_serve(stringify!($name), <$server_ty>::new(), (r, w));
        }
        (stringify!($name), spawn as SpawnServerFn)
    }};
}

pub static BUILTIN_EXTENSIONS: Lazy<HashMap<&'static str, SpawnServerFn>> = Lazy::new(|| {
    HashMap::from([
        builtin!(autovisualiser, AutoVisualiserRouter),
        builtin!(computercontroller, ComputerControllerServer),
        builtin!(memory, MemoryServer),
        builtin!(repomix, RepomixServer),
        builtin!(searxng, SearxngServer),
        builtin!(tutorial, TutorialServer),
    ])
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repomix_is_registered_as_a_builtin_extension() {
        assert!(
            BUILTIN_EXTENSIONS.contains_key("repomix"),
            "repomix must be registered in BUILTIN_EXTENSIONS to be loadable in-agent"
        );
    }
}
