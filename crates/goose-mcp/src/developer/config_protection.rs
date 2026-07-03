//! Guards goose's own configuration directory against writes/edits through the
//! developer extension's file tools. Workspace path confinement (`allowed_paths`)
//! is opt-in and scoped to a project directory; this check is unconditional and
//! specifically prevents a session from overwriting goose's own `config.yaml` /
//! `secrets.yaml`, regardless of what confinement is otherwise in effect.

use etcetera::{choose_app_strategy, AppStrategy, AppStrategyArgs};
use std::path::{Path, PathBuf};

fn goose_config_dir() -> Option<PathBuf> {
    if let Ok(test_root) = std::env::var("GOOSE_PATH_ROOT") {
        return Some(PathBuf::from(test_root).join("config"));
    }
    choose_app_strategy(AppStrategyArgs {
        top_level_domain: "Block".to_string(),
        author: "Block".to_string(),
        app_name: "goose-plus".to_string(),
    })
    .ok()
    .map(|strategy| strategy.config_dir())
}

/// True if `path` resolves into goose's own configuration directory. Canonicalizes
/// the nearest existing ancestor (following symlinks, so it can't be bypassed via a
/// symlink pointing into the config directory) — walking upward past however many
/// not-yet-created path components a new file/directory has.
pub fn is_goose_config_path(path: &Path) -> bool {
    let Some(config_dir) = goose_config_dir() else {
        return false;
    };
    let Ok(canonical_config_dir) = config_dir.canonicalize() else {
        return false;
    };

    let mut candidate = path.to_path_buf();
    loop {
        if let Ok(canonical) = candidate.canonicalize() {
            return canonical.starts_with(&canonical_config_dir);
        }
        match candidate.parent() {
            Some(parent) if parent != candidate => candidate = parent.to_path_buf(),
            _ => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;

    // All tests here mutate the process-global GOOSE_PATH_ROOT env var, so they
    // must run serially — otherwise a concurrently-running test can see another
    // test's temp dir mid-check.
    fn with_test_config_root<F: FnOnce(&Path)>(f: F) {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("config");
        fs::create_dir_all(&config_dir).unwrap();
        // SAFETY: single-threaded test body; no other thread reads this env var
        // concurrently within this process during the closure's execution.
        unsafe {
            std::env::set_var("GOOSE_PATH_ROOT", temp_dir.path());
        }
        f(&config_dir);
        unsafe {
            std::env::remove_var("GOOSE_PATH_ROOT");
        }
    }

    #[test]
    #[serial]
    fn rejects_existing_file_inside_config_dir() {
        with_test_config_root(|config_dir| {
            let config_yaml = config_dir.join("config.yaml");
            fs::write(&config_yaml, "existing: true").unwrap();
            assert!(is_goose_config_path(&config_yaml));
        });
    }

    #[test]
    #[serial]
    fn rejects_new_file_inside_config_dir() {
        with_test_config_root(|config_dir| {
            let secrets_yaml = config_dir.join("secrets.yaml");
            assert!(!secrets_yaml.exists());
            assert!(is_goose_config_path(&secrets_yaml));
        });
    }

    #[test]
    #[serial]
    fn rejects_nested_new_file_inside_config_dir() {
        with_test_config_root(|config_dir| {
            let nested = config_dir.join("subdir").join("new_file.txt");
            // Parent doesn't exist either — falls back past it, but the
            // furthest existing ancestor (config_dir itself) still resolves
            // inside the config directory.
            assert!(!nested.parent().unwrap().exists());
            assert!(is_goose_config_path(&nested));
        });
    }

    #[test]
    #[serial]
    fn allows_file_outside_config_dir() {
        with_test_config_root(|_config_dir| {
            let temp_dir = TempDir::new().unwrap();
            let outside_file = temp_dir.path().join("test.txt");
            fs::write(&outside_file, "test").unwrap();
            assert!(!is_goose_config_path(&outside_file));
        });
    }
}
