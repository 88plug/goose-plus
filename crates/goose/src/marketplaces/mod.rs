//! Claude Code plugin marketplaces.
//!
//! A marketplace repository carries no plugin of its own — it publishes a
//! `.claude-plugin/marketplace.json` catalog pointing at the repositories that
//! do. Installing from one therefore means resolving a name through the catalog
//! and then installing the repository it names.

use crate::config::Config;
use anyhow::{bail, Result};
use fs_err as fs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

pub const DEFAULT_MARKETPLACES: &[&str] = &[
    "https://github.com/88plug/claude-code-plugins",
    "https://github.com/anthropics/claude-plugins-official",
];

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

/// A catalog entry's location: either a bare string or an object.
///
/// Deliberately permissive. The object form is captured field-by-field rather
/// than as a tagged enum over known `source` discriminators, because a strict
/// enum fails the *whole* catalog the moment a new one appears — exactly what
/// `git-subdir` did to clients validating against the published schema
/// (anthropics/claude-plugins-official#585).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum EntrySource {
    Object(SourceObject),
    Plain(String),
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SourceObject {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    /// `github` shorthand, `owner/repo`.
    #[serde(default)]
    pub repo: Option<String>,
    /// Subdirectory holding the plugin, for `git-subdir` and `url` entries.
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default, rename = "ref")]
    pub git_ref: Option<String>,
}

/// Where a catalog entry's plugin actually lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedSource {
    /// Clone `url`, then install from `subdir` within it when set.
    Git { url: String, subdir: Option<String> },
    /// A path inside the marketplace checkout itself.
    Local { path: String },
}

impl EntrySource {
    pub fn resolve(&self) -> Option<ResolvedSource> {
        match self {
            Self::Plain(value) => {
                if value.starts_with("http") || value.starts_with("git@") {
                    Some(ResolvedSource::Git {
                        url: value.clone(),
                        subdir: None,
                    })
                } else {
                    // Relative to the marketplace repo, e.g. "./plugins/foo".
                    Some(ResolvedSource::Local {
                        path: value.clone(),
                    })
                }
            }
            Self::Object(obj) => {
                if let Some(url) = &obj.url {
                    return Some(ResolvedSource::Git {
                        url: url.clone(),
                        subdir: obj.path.clone(),
                    });
                }
                if let Some(repo) = &obj.repo {
                    return Some(ResolvedSource::Git {
                        url: format!("https://github.com/{repo}"),
                        subdir: obj.path.clone(),
                    });
                }
                obj.path
                    .as_ref()
                    .map(|path| ResolvedSource::Local { path: path.clone() })
            }
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

/// Registered marketplaces, keyed by marketplace name, stored under
/// `marketplaces` in config.yaml alongside the existing `plugins` map.
const MARKETPLACES_CONFIG_KEY: &str = "marketplaces";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceRecord {
    /// The source as given, after shorthand expansion.
    pub source: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub plugin_count: usize,
}

/// Expand the shorthands Claude Code accepts: `owner/repo` means GitHub, and a
/// URL or filesystem path is used as given.
pub fn normalize_source(source: &str) -> String {
    let source = source.trim();
    if source.contains("://")
        || source.starts_with("git@")
        || source.starts_with('.')
        || source.starts_with('/')
        || source.starts_with('~')
    {
        return source.to_string();
    }
    // `owner/repo` or `owner/repo@ref`
    if source.matches('/').count() == 1 {
        return format!("https://github.com/{source}");
    }
    source.to_string()
}

pub fn registered() -> HashMap<String, MarketplaceRecord> {
    Config::global()
        .get_param(MARKETPLACES_CONFIG_KEY)
        .unwrap_or_default()
}

pub fn register(name: &str, record: MarketplaceRecord) -> Result<()> {
    let mut all = registered();
    all.insert(name.to_string(), record);
    Config::global().set_param(MARKETPLACES_CONFIG_KEY, all)?;
    Ok(())
}

pub fn unregister(name: &str) -> Result<bool> {
    let mut all = registered();
    let removed = all.remove(name).is_some();
    if removed {
        Config::global().set_param(MARKETPLACES_CONFIG_KEY, all)?;
    }
    Ok(removed)
}

/// Every marketplace source to search: those registered by the user first, then
/// the built-in defaults that are not already registered.
pub fn search_sources() -> Vec<String> {
    let mut sources: Vec<String> = registered().values().map(|r| r.source.clone()).collect();
    for default in DEFAULT_MARKETPLACES {
        if !sources.iter().any(|s| s == default) {
            sources.push((*default).to_string());
        }
    }
    sources
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

/// Resolve a catalog entry to where its plugin can actually be fetched from.
pub fn resolve_entry(marketplace: &Marketplace, name: &str) -> Result<ResolvedSource> {
    let Some(entry) = marketplace.find(name) else {
        bail!(
            "Plugin '{}' is not in this marketplace. Available: {}",
            name,
            marketplace.names().join(", ")
        );
    };

    entry.source.resolve().ok_or_else(|| {
        anyhow::anyhow!(
            "Plugin '{}' has no installable source in this marketplace",
            name
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every source shape present in anthropics/claude-plugins-official.
    const CATALOG: &str = r#"{
        "name": "example",
        "description": "d",
        "owner": {"name": "o"},
        "plugins": [
          {"name":"url","source":{"source":"url","url":"https://example.invalid/u.git","sha":"a"},
           "description":"d","category":"c","tags":[],"version":"1.0.0"},
          {"name":"url-path","source":{"source":"url","url":"https://example.invalid/u.git","path":"plugin","sha":"a"}},
          {"name":"git-subdir","source":{"source":"git-subdir","url":"https://example.invalid/m.git","path":"plugin","ref":"main","sha":"a"}},
          {"name":"github","source":{"source":"github","repo":"owner/repo","commit":"c","sha":"a"}},
          {"name":"relative","source":"./plugins/relative"},
          {"name":"bare","source":"https://example.invalid/bare.git"}
        ]
    }"#;

    fn catalog(dir: &Path) -> Marketplace {
        fs::create_dir_all(dir.join(".claude-plugin")).unwrap();
        fs::write(dir.join(MARKETPLACE_MANIFEST), CATALOG).unwrap();
        read_marketplace(dir).unwrap()
    }

    fn git(url: &str, subdir: Option<&str>) -> ResolvedSource {
        ResolvedSource::Git {
            url: url.to_string(),
            subdir: subdir.map(str::to_string),
        }
    }

    #[test]
    fn parses_every_source_shape_without_discarding_the_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let m = catalog(dir.path());
        // An unknown discriminator must not sink the whole file, which is what
        // git-subdir did to strict clients.
        assert_eq!(m.plugins.len(), 6);
        assert!(is_marketplace(dir.path()));
    }

    #[test]
    fn resolves_url_github_and_subdir_sources() {
        let dir = tempfile::tempdir().unwrap();
        let m = catalog(dir.path());
        assert_eq!(
            resolve_entry(&m, "url").unwrap(),
            git("https://example.invalid/u.git", None)
        );
        assert_eq!(
            resolve_entry(&m, "url-path").unwrap(),
            git("https://example.invalid/u.git", Some("plugin"))
        );
        assert_eq!(
            resolve_entry(&m, "git-subdir").unwrap(),
            git("https://example.invalid/m.git", Some("plugin"))
        );
        assert_eq!(
            resolve_entry(&m, "github").unwrap(),
            git("https://github.com/owner/repo", None)
        );
        assert_eq!(
            resolve_entry(&m, "bare").unwrap(),
            git("https://example.invalid/bare.git", None)
        );
    }

    #[test]
    fn resolves_relative_source_against_the_marketplace_checkout() {
        let dir = tempfile::tempdir().unwrap();
        let m = catalog(dir.path());
        assert_eq!(
            resolve_entry(&m, "relative").unwrap(),
            ResolvedSource::Local {
                path: "./plugins/relative".to_string()
            }
        );
    }

    #[test]
    fn unknown_plugin_lists_what_is_available() {
        let dir = tempfile::tempdir().unwrap();
        let m = catalog(dir.path());
        let err = resolve_entry(&m, "nope").unwrap_err().to_string();
        assert!(err.contains("bare, git-subdir, github"), "{err}");
    }

    #[test]
    fn a_repo_without_a_catalog_is_not_a_marketplace() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_marketplace(dir.path()).is_none());
        assert!(!is_marketplace(dir.path()));
    }
}
