//! Claude Code plugin marketplaces.
//!
//! A marketplace repository carries no plugin of its own — it publishes a
//! `.claude-plugin/marketplace.json` catalog pointing at the repositories that
//! do. Installing from one therefore means resolving a name through the catalog
//! and then installing the repository it names.

use anyhow::{bail, Result};
use fs_err as fs;
use serde::Deserialize;
use std::path::Path;

pub const DEFAULT_MARKETPLACES: &[&str] = &["https://github.com/88plug/claude-code-plugins"];

pub const MARKETPLACE_MANIFEST: &str = ".claude-plugin/marketplace.json";

#[derive(Debug, Clone, Deserialize)]
pub struct Marketplace {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub plugins: Vec<MarketplaceEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MarketplaceEntry {
    pub name: String,
    pub source: EntrySource,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
}

/// A catalog entry's location. The schema allows either a bare string or an
/// object; only a URL is something we can clone.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum EntrySource {
    Url { url: String },
    Plain(String),
}

impl EntrySource {
    pub fn git_url(&self) -> Option<&str> {
        match self {
            Self::Url { url } => Some(url.as_str()),
            // A bare string is only usable when it is itself a URL; a relative
            // in-repo path is not something we can clone.
            Self::Plain(value) if value.starts_with("http") => Some(value.as_str()),
            Self::Plain(_) => None,
        }
    }
}

impl Marketplace {
    pub fn find(&self, name: &str) -> Option<&MarketplaceEntry> {
        self.plugins.iter().find(|entry| entry.name == name)
    }

    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.plugins.iter().map(|e| e.name.as_str()).collect();
        names.sort_unstable();
        names
    }
}

/// Read the catalog from a checked-out marketplace repository.
pub fn read_marketplace(checkout_dir: &Path) -> Option<Marketplace> {
    let manifest = checkout_dir.join(MARKETPLACE_MANIFEST);
    if !manifest.is_file() {
        return None;
    }
    serde_json::from_str(&fs::read_to_string(&manifest).ok()?).ok()
}

/// True when a checkout publishes a non-empty catalog.
pub fn is_marketplace(checkout_dir: &Path) -> bool {
    read_marketplace(checkout_dir).is_some_and(|m| !m.plugins.is_empty())
}

/// Resolve a catalog entry to the git URL that should be cloned.
pub fn resolve_entry_url(marketplace: &Marketplace, name: &str) -> Result<String> {
    let Some(entry) = marketplace.find(name) else {
        bail!(
            "Plugin '{}' is not in this marketplace. Available: {}",
            name,
            marketplace.names().join(", ")
        );
    };

    match entry.source.git_url() {
        Some(url) => Ok(url.to_string()),
        None => bail!(
            "Plugin '{}' has no installable source URL in this marketplace",
            name
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CATALOG: &str = r#"{
        "name": "example",
        "description": "d",
        "owner": {"name": "o"},
        "plugins": [
          {"name":"ooda","source":{"source":"url","url":"https://example.invalid/ooda.git"},
           "description":"loop","category":"c","tags":[],"version":"1.0.0"},
          {"name":"bare","source":"https://example.invalid/bare.git"},
          {"name":"local","source":"./in-repo"}
        ]
    }"#;

    fn catalog(dir: &Path) -> Marketplace {
        fs::create_dir_all(dir.join(".claude-plugin")).unwrap();
        fs::write(dir.join(MARKETPLACE_MANIFEST), CATALOG).unwrap();
        read_marketplace(dir).unwrap()
    }

    #[test]
    fn reads_catalog_and_resolves_object_source() {
        let dir = tempfile::tempdir().unwrap();
        let m = catalog(dir.path());
        assert_eq!(m.plugins.len(), 3);
        assert!(is_marketplace(dir.path()));
        assert_eq!(
            resolve_entry_url(&m, "ooda").unwrap(),
            "https://example.invalid/ooda.git"
        );
    }

    #[test]
    fn resolves_bare_string_source_but_not_relative_paths() {
        let dir = tempfile::tempdir().unwrap();
        let m = catalog(dir.path());
        assert_eq!(
            resolve_entry_url(&m, "bare").unwrap(),
            "https://example.invalid/bare.git"
        );
        assert!(resolve_entry_url(&m, "local").is_err());
    }

    #[test]
    fn unknown_plugin_lists_what_is_available() {
        let dir = tempfile::tempdir().unwrap();
        let m = catalog(dir.path());
        let err = resolve_entry_url(&m, "nope").unwrap_err().to_string();
        assert!(err.contains("bare, local, ooda"), "{err}");
    }

    #[test]
    fn a_repo_without_a_catalog_is_not_a_marketplace() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_marketplace(dir.path()).is_none());
        assert!(!is_marketplace(dir.path()));
    }
}
