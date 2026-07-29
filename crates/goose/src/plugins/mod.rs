pub mod discovery;
pub mod formats;
pub mod mcp_servers;

use crate::config::paths::Paths;
use crate::subprocess::SubprocessExt;
use anyhow::{anyhow, bail, Result};
use chrono::{DateTime, Duration, Utc};
use fs_err as fs;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::warn;

const INSTALL_METADATA: &str = ".goose-plugin-install.json";
const AUTO_UPDATE_INTERVAL_HOURS: i64 = 24;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginFormat {
    Gemini,
    OpenPlugins,
}

impl std::fmt::Display for PluginFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginFormat::Gemini => write!(f, "gemini"),
            PluginFormat::OpenPlugins => write!(f, "open-plugins"),
        }
    }
}

pub fn plugin_install_dir() -> PathBuf {
    Paths::plugins_dir()
}

pub fn project_plugin_install_dir(project_root: &Path) -> PathBuf {
    project_root.join(".agents").join("plugins")
}

#[derive(Debug, Clone)]
pub struct PluginInstall {
    pub name: String,
    pub version: String,
    pub format: PluginFormat,
    pub source: String,
    pub directory: PathBuf,
    pub skills: Vec<ImportedSkill>,
}

#[derive(Debug, Clone, Default)]
pub struct PluginInstallOptions {
    pub auto_update: bool,
}

#[derive(Debug)]
pub struct PluginAutoUpdateResult {
    pub name: String,
    pub result: Result<PluginInstall>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedSkill {
    pub name: String,
    pub directory: PathBuf,
}

#[derive(Debug, thiserror::Error)]
#[error("format not supported")]
pub struct FormatNotSupported;

#[derive(Debug, Deserialize, Serialize)]
struct InstallMetadata {
    source: String,
    source_type: String,
    #[allow(dead_code)]
    format: String,
    #[serde(default)]
    auto_update: bool,
    #[serde(default)]
    last_update_check: Option<DateTime<Utc>>,
}

pub fn installed_plugin_skill_dirs() -> Vec<PathBuf> {
    let plugins_dir = plugin_install_dir();
    for update in auto_update_plugins_at_root(Utc::now(), &plugins_dir) {
        if let Err(err) = update.result {
            warn!(
                "Failed to auto-update plugin '{}': {}. Using currently installed version.",
                update.name, err
            );
        }
    }

    let entries = match fs::read_dir(plugins_dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut seen = HashSet::new();
    entries
        .flatten()
        .flat_map(|entry| {
            let plugin_dir = entry.path();
            let default_skills_dir = plugin_dir.join("skills");
            let mut skill_dirs = Vec::new();
            if default_skills_dir.is_dir() {
                skill_dirs.push(default_skills_dir);
            }
            skill_dirs.extend(formats::open_plugins::installed_skill_dirs(&plugin_dir));
            skill_dirs
        })
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

pub fn install_plugin(source: &str) -> Result<PluginInstall> {
    install_plugin_with_options(source, PluginInstallOptions::default())
}

pub fn install_plugin_with_options(
    source: &str,
    options: PluginInstallOptions,
) -> Result<PluginInstall> {
    install_plugin_with_options_at_root(source, options, &plugin_install_dir())
}

fn install_plugin_with_options_at_root(
    source: &str,
    options: PluginInstallOptions,
    install_root: &Path,
) -> Result<PluginInstall> {
    let source = source.trim();
    if source.is_empty() {
        bail!("Plugin source URL must not be empty");
    }

    // `name@marketplace` and a bare `name` both resolve through marketplaces,
    // matching `claude plugin install <plugin>@<marketplace>`.
    if let Some((name, marketplace)) = split_qualified_name(source) {
        return install_named_from_marketplaces(name, Some(marketplace), options, install_root);
    }
    if !looks_like_git_source(source) {
        return install_named_from_marketplaces(source, None, options, install_root);
    }

    let source = source.to_string();
    let temp_dir = tempfile::tempdir()?;
    let checkout_dir = temp_dir.path().join("checkout");
    clone_git_repo(&source, &checkout_dir)?;

    // A marketplace repo has no plugin of its own; installing it would
    // otherwise fail with an opaque "no supported plugin format".
    if crate::marketplaces::is_marketplace(&checkout_dir)
        && !formats::has_installable_plugin(&checkout_dir)
    {
        let marketplace = crate::marketplaces::read_marketplace(&checkout_dir)
            .expect("is_marketplace checked the manifest parses");
        bail!(
            "{} is a plugin marketplace, not a plugin. Install one of its {} plugins by name, \
             e.g. `goose plugin install {}`.\n\nAvailable: {}",
            source,
            marketplace.plugins.len(),
            marketplace.names().first().unwrap_or(&"<name>"),
            marketplace.names().join(", ")
        );
    }

    install_from_checkout_at_root(
        &source,
        &checkout_dir,
        install_root,
        &options,
        options.auto_update.then_some(Utc::now()),
    )
}

/// Clone a marketplace and register it, mirroring `claude plugin marketplace add`.
pub fn marketplace_add(source: &str) -> Result<(String, crate::marketplaces::MarketplaceRecord)> {
    use crate::marketplaces as mkt;

    let normalized = mkt::normalize_source(source);
    let temp_dir = tempfile::tempdir()?;
    let checkout = temp_dir.path().join("marketplace");
    clone_marketplace(&normalized, &checkout)?;

    let Some(marketplace) = mkt::read_marketplace(&checkout) else {
        bail!(
            "{} does not publish a {}",
            normalized,
            mkt::MARKETPLACE_MANIFEST
        );
    };

    let name = marketplace
        .name
        .clone()
        .unwrap_or_else(|| infer_marketplace_name(&normalized));
    let record = mkt::MarketplaceRecord {
        source: normalized,
        description: marketplace.description.clone(),
        plugin_count: marketplace.plugins.len(),
    };
    mkt::register(&name, record.clone())?;
    Ok((name, record))
}

pub fn marketplace_remove(name: &str) -> Result<bool> {
    crate::marketplaces::unregister(name)
}

/// Re-read a registered marketplace so its cached counts reflect the remote.
pub fn marketplace_update(name: &str) -> Result<crate::marketplaces::MarketplaceRecord> {
    let Some(existing) = crate::marketplaces::registered().get(name).cloned() else {
        bail!("Marketplace '{}' is not registered", name);
    };
    let (_, record) = marketplace_add(&existing.source)?;
    Ok(record)
}

/// List every plugin offered by a marketplace, or by all registered ones.
pub fn marketplace_plugins(
    name: Option<&str>,
) -> Result<Vec<(String, crate::marketplaces::Marketplace)>> {
    use crate::marketplaces as mkt;

    let sources: Vec<(String, String)> = match name {
        Some(name) => {
            let Some(record) = mkt::registered().get(name).cloned() else {
                bail!("Marketplace '{}' is not registered", name);
            };
            vec![(name.to_string(), record.source)]
        }
        None => mkt::registered()
            .into_iter()
            .map(|(n, r)| (n, r.source))
            .chain(
                mkt::DEFAULT_MARKETPLACES
                    .iter()
                    .map(|s| ((*s).to_string(), (*s).to_string())),
            )
            .collect(),
    };

    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for (label, source) in sources {
        if !seen.insert(source.clone()) {
            continue;
        }
        let temp_dir = tempfile::tempdir()?;
        let checkout = temp_dir.path().join("marketplace");
        if clone_marketplace(&source, &checkout).is_err() {
            continue;
        }
        if let Some(marketplace) = crate::marketplaces::read_marketplace(&checkout) {
            let label = marketplace.name.clone().unwrap_or(label);
            out.push((label, marketplace));
        }
    }
    Ok(out)
}

/// A marketplace may be a git remote or a local directory.
fn clone_marketplace(source: &str, destination: &Path) -> Result<()> {
    let local = Path::new(source);
    if local.is_dir() {
        copy_dir_all(local, destination)?;
        return Ok(());
    }
    clone_git_repo(source, destination)
}

fn infer_marketplace_name(source: &str) -> String {
    source
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or("marketplace")
        .to_string()
}

/// Split `plugin@marketplace`. A URL is never split, so `git@host:repo` and
/// `https://…` sources are unaffected.
fn split_qualified_name(source: &str) -> Option<(&str, &str)> {
    if source.contains("://") || source.starts_with("git@") {
        return None;
    }
    let (name, marketplace) = source.rsplit_once('@')?;
    if name.is_empty() || marketplace.is_empty() {
        return None;
    }
    Some((name, marketplace))
}

/// Anything that looks like a URL or a local path is used verbatim; everything
/// else is treated as a marketplace plugin name.
fn looks_like_git_source(source: &str) -> bool {
    source.contains("://")
        || source.starts_with("git@")
        || source.starts_with('.')
        || source.starts_with('/')
        || source.contains('/')
}

/// Resolve `name` through the default marketplaces and install what it points
/// at. A relative source lives inside the marketplace checkout, so the checkout
/// has to outlive resolution — hence installing here rather than returning a URL.
fn install_named_from_marketplaces(
    name: &str,
    from_marketplace: Option<&str>,
    options: PluginInstallOptions,
    install_root: &Path,
) -> Result<PluginInstall> {
    use crate::marketplaces::{read_marketplace, resolve_entry, ResolvedSource};

    let sources = match from_marketplace {
        Some(wanted) => {
            let registered = crate::marketplaces::registered();
            match registered.get(wanted) {
                Some(record) => vec![record.source.clone()],
                // Not registered: accept it as a source directly, so
                // `name@owner/repo` works without an explicit add.
                None => vec![crate::marketplaces::normalize_source(wanted)],
            }
        }
        None => crate::marketplaces::search_sources(),
    };

    let mut errors = Vec::new();

    for marketplace_url in &sources {
        let temp_dir = tempfile::tempdir()?;
        let marketplace_dir = temp_dir.path().join("marketplace");
        if let Err(err) = clone_marketplace(marketplace_url, &marketplace_dir) {
            errors.push(format!("{marketplace_url}: {err}"));
            continue;
        }

        let Some(marketplace) = read_marketplace(&marketplace_dir) else {
            errors.push(format!("{marketplace_url}: no marketplace manifest"));
            continue;
        };

        let resolved = match resolve_entry(&marketplace, name) {
            Ok(resolved) => resolved,
            Err(err) => {
                errors.push(err.to_string());
                continue;
            }
        };

        let (source, plugin_dir, _clone_guard) = match resolved {
            ResolvedSource::Git { url, subdir } => {
                let clone_dir = tempfile::tempdir()?;
                let checkout = clone_dir.path().join("checkout");
                clone_git_repo(&url, &checkout)?;
                let dir = match &subdir {
                    Some(path) => join_subdir(&checkout, path)?,
                    None => checkout,
                };
                (url, dir, Some(clone_dir))
            }
            ResolvedSource::Local { path } => (
                marketplace_url.clone(),
                join_subdir(&marketplace_dir, &path)?,
                None,
            ),
        };

        return install_from_checkout_at_root(
            &source,
            &plugin_dir,
            install_root,
            &options,
            options.auto_update.then_some(Utc::now()),
        );
    }

    bail!(
        "Could not resolve plugin '{}' from any marketplace.\n{}",
        name,
        errors.join("\n")
    )
}

/// Join a marketplace-supplied relative path, refusing anything that escapes
/// the checkout.
fn join_subdir(root: &Path, relative: &str) -> Result<PathBuf> {
    let trimmed = relative.trim_start_matches("./");
    let candidate = root.join(trimmed);
    if trimmed.is_empty()
        || Path::new(trimmed).is_absolute()
        || Path::new(trimmed)
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        bail!(
            "Marketplace source path '{}' escapes the checkout",
            relative
        );
    }
    if !candidate.is_dir() {
        bail!(
            "Marketplace source path '{}' does not exist in the repository",
            relative
        );
    }
    Ok(candidate)
}

#[derive(Debug, Clone)]
pub struct InstalledPlugin {
    pub name: String,
    pub source: String,
    pub directory: PathBuf,
}

/// Every plugin present in the install directory, with the source it came from.
pub fn installed_plugins() -> Result<Vec<InstalledPlugin>> {
    let root = plugin_install_dir();
    let Ok(entries) = fs::read_dir(&root) else {
        return Ok(Vec::new());
    };

    let mut plugins: Vec<InstalledPlugin> = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| {
            let directory = entry.path();
            let source = read_install_metadata(&directory)
                .map(|m| m.source)
                .unwrap_or_else(|_| "<unknown source>".to_string());
            InstalledPlugin {
                name: entry.file_name().to_string_lossy().into_owned(),
                source,
                directory,
            }
        })
        .collect();

    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(plugins)
}

pub fn uninstall_plugin(name: &str) -> Result<()> {
    let directory = plugin_install_dir().join(name);
    if !directory.is_dir() {
        bail!("Plugin '{}' is not installed", name);
    }
    fs::remove_dir_all(&directory)?;
    Ok(())
}

pub fn update_plugin(name: &str) -> Result<PluginInstall> {
    update_plugin_at_root(Utc::now(), &plugin_install_dir(), name)
}

pub fn auto_update_plugins() -> Vec<PluginAutoUpdateResult> {
    auto_update_plugins_at_root(Utc::now(), &plugin_install_dir())
}

fn auto_update_plugins_at_root(
    now: DateTime<Utc>,
    plugins_dir: &Path,
) -> Vec<PluginAutoUpdateResult> {
    let entries = match fs::read_dir(plugins_dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    entries
        .flatten()
        .filter_map(|entry| {
            let plugin_dir = entry.path();
            if !plugin_dir.is_dir() {
                return None;
            }

            let name = entry.file_name().to_string_lossy().into_owned();
            let metadata = match read_install_metadata(&plugin_dir) {
                Ok(metadata) => metadata,
                Err(_) => return None,
            };

            if !metadata.auto_update || metadata.source_type != "git" {
                return None;
            }

            if !should_auto_update(now, metadata.last_update_check) {
                return None;
            }

            let result = mark_last_update_check(&plugin_dir, now)
                .and_then(|_| update_plugin_at_root(now, plugins_dir, &name));

            Some(PluginAutoUpdateResult {
                name: name.clone(),
                result,
            })
        })
        .collect()
}

fn should_auto_update(now: DateTime<Utc>, last_update_check: Option<DateTime<Utc>>) -> bool {
    last_update_check
        .is_none_or(|checked_at| now - checked_at >= Duration::hours(AUTO_UPDATE_INTERVAL_HOURS))
}

fn update_plugin_at_root(
    now: DateTime<Utc>,
    install_root: &Path,
    name: &str,
) -> Result<PluginInstall> {
    if name.trim().is_empty() {
        bail!("Plugin name must not be empty");
    }

    let current_install_dir = install_root.join(name);
    if !current_install_dir.is_dir() {
        bail!("Plugin '{}' is not installed", name);
    }

    let metadata = read_install_metadata(&current_install_dir)?;
    if metadata.source_type != "git" {
        bail!(
            "Plugin '{}' was installed from '{}' and cannot be updated with this command",
            name,
            metadata.source_type
        );
    }

    fs::create_dir_all(install_root)?;
    let temp_dir = tempfile::tempdir_in(install_root)?;
    let checkout_dir = temp_dir.path().join("checkout");
    clone_git_repo(&metadata.source, &checkout_dir)?;

    let options = PluginInstallOptions {
        auto_update: metadata.auto_update,
    };
    let updated = install_from_checkout_at_root(
        &metadata.source,
        &checkout_dir,
        temp_dir.path(),
        &options,
        Some(now),
    )?;
    if updated.name != name {
        bail!(
            "Updated plugin name '{}' does not match installed plugin '{}'",
            updated.name,
            name
        );
    }

    replace_plugin_dir(&updated.directory, &current_install_dir)?;

    Ok(PluginInstall {
        directory: current_install_dir,
        ..updated
    })
}

fn install_from_checkout_at_root(
    source: &str,
    checkout_dir: &Path,
    install_root: &Path,
    options: &PluginInstallOptions,
    last_update_check: Option<DateTime<Utc>>,
) -> Result<PluginInstall> {
    match formats::open_plugins::try_install_from_manifest_at_root(
        source,
        checkout_dir,
        install_root,
        options,
        last_update_check,
    ) {
        Ok(install) => return Ok(install),
        Err(err) if err.is::<FormatNotSupported>() => {}
        Err(err) => return Err(err),
    }

    match formats::gemini::try_install_from_manifest_at_root(
        source,
        checkout_dir,
        install_root,
        options,
        last_update_check,
    ) {
        Ok(install) => Ok(install),
        Err(err) if err.is::<FormatNotSupported>() => bail!("No supported plugin format found"),
        Err(err) => Err(err),
    }
}

fn clone_git_repo(source: &str, destination: &Path) -> Result<()> {
    let output = Command::new("git")
        .arg("clone")
        .arg("--depth")
        .arg("1")
        .arg(source)
        .arg(destination)
        .set_no_window()
        .output()
        .map_err(|e| anyhow!("Failed to run git clone: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let message = if stderr.is_empty() { stdout } else { stderr };
        bail!("Failed to clone plugin repository: {message}");
    }

    Ok(())
}

fn read_install_metadata(directory: &Path) -> Result<InstallMetadata> {
    let metadata_path = directory.join(INSTALL_METADATA);
    if !metadata_path.is_file() {
        bail!(
            "Plugin at {} does not contain install metadata and cannot be updated",
            directory.display()
        );
    }

    Ok(serde_json::from_str(&fs::read_to_string(metadata_path)?)?)
}

fn mark_last_update_check(directory: &Path, checked_at: DateTime<Utc>) -> Result<()> {
    let mut metadata = read_install_metadata(directory)?;
    metadata.last_update_check = Some(checked_at);
    fs::write(
        directory.join(INSTALL_METADATA),
        serde_json::to_string_pretty(&metadata)?,
    )?;
    Ok(())
}

fn write_install_metadata(
    destination: &Path,
    source: &str,
    format: &str,
    auto_update: bool,
    last_update_check: Option<DateTime<Utc>>,
) -> Result<()> {
    let metadata = InstallMetadata {
        source: source.to_string(),
        source_type: "git".to_string(),
        format: format.to_string(),
        auto_update,
        last_update_check,
    };
    fs::write(
        destination.join(INSTALL_METADATA),
        serde_json::to_string_pretty(&metadata)?,
    )?;
    Ok(())
}

fn replace_plugin_dir(source: &Path, destination: &Path) -> Result<()> {
    let parent = destination
        .parent()
        .ok_or_else(|| anyhow!("Plugin destination has no parent directory"))?;
    let backup_dir = tempfile::tempdir_in(parent)?;
    let backup_plugin_dir = backup_dir.path().join("plugin");

    fs::rename(destination, &backup_plugin_dir)?;
    if let Err(err) = fs::rename(source, destination) {
        fs::rename(&backup_plugin_dir, destination)?;
        return Err(err.into());
    }

    Ok(())
}

fn copy_dir_all(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            copy_dir_all(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_repo_without_supported_manifest() {
        let install_root = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();

        let err = install_from_checkout_at_root(
            "https://example.invalid/repo.git",
            repo.path(),
            install_root.path(),
            &PluginInstallOptions::default(),
            None,
        )
        .unwrap_err();

        assert!(err.to_string().contains("No supported plugin format found"));
    }

    #[test]
    fn updates_git_backed_plugin() {
        let install_root = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        write_gemini_plugin(repo.path(), "1.0.0", "Audit code");
        init_git_repo(repo.path());
        commit_git_repo(repo.path(), "initial");
        let source = repo.path().to_path_buf();

        let installed = install_plugin_with_options_at_root(
            source.to_str().unwrap(),
            PluginInstallOptions::default(),
            install_root.path(),
        )
        .unwrap();
        assert_eq!(installed.version, "1.0.0");

        write_gemini_plugin(&source, "2.0.0", "Audit updated code");
        commit_git_repo(&source, "update");

        let updated =
            update_plugin_at_root(Utc::now(), install_root.path(), "test-plugin").unwrap();

        assert_eq!(updated.version, "2.0.0");
        assert_eq!(updated.directory, install_root.path().join("test-plugin"));
        assert_eq!(
            fs::read_to_string(updated.directory.join("skills/audit/SKILL.md")).unwrap(),
            "---\nname: audit\ndescription: Audit updated code\n---\nDo an audit."
        );
    }

    #[test]
    fn auto_update_plugins_updates_enabled_plugins() {
        let install_root = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        write_gemini_plugin(repo.path(), "1.0.0", "Audit code");
        init_git_repo(repo.path());
        commit_git_repo(repo.path(), "initial");
        let source = repo.path().to_path_buf();

        let installed = install_plugin_with_options_at_root(
            source.to_str().unwrap(),
            PluginInstallOptions { auto_update: true },
            install_root.path(),
        )
        .unwrap();
        let old_check = Utc::now() - Duration::hours(AUTO_UPDATE_INTERVAL_HOURS + 1);
        mark_last_update_check(&installed.directory, old_check).unwrap();

        write_gemini_plugin(&source, "2.0.0", "Audit updated code");
        commit_git_repo(&source, "update");

        let updates = auto_update_plugins_at_root(Utc::now(), install_root.path());

        assert_eq!(updates.len(), 1);
        assert!(updates[0].result.is_ok());
        assert_eq!(
            fs::read_to_string(installed.directory.join("skills/audit/SKILL.md")).unwrap(),
            "---\nname: audit\ndescription: Audit updated code\n---\nDo an audit."
        );
        let metadata = read_install_metadata(&installed.directory).unwrap();
        assert!(metadata.auto_update);
        assert!(metadata.last_update_check.unwrap() > old_check);
    }

    #[test]
    fn auto_update_plugins_skips_recently_checked_plugins() {
        let install_root = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        write_gemini_plugin(repo.path(), "1.0.0", "Audit code");
        init_git_repo(repo.path());
        commit_git_repo(repo.path(), "initial");
        let source = repo.path().to_path_buf();

        let installed = install_plugin_with_options_at_root(
            source.to_str().unwrap(),
            PluginInstallOptions { auto_update: true },
            install_root.path(),
        )
        .unwrap();
        let recent_check = Utc::now();
        mark_last_update_check(&installed.directory, recent_check).unwrap();

        write_gemini_plugin(&source, "2.0.0", "Audit updated code");
        commit_git_repo(&source, "update");

        let updates =
            auto_update_plugins_at_root(recent_check + Duration::hours(1), install_root.path());

        assert!(updates.is_empty());
        assert_eq!(
            fs::read_to_string(installed.directory.join("skills/audit/SKILL.md")).unwrap(),
            "---\nname: audit\ndescription: Audit code\n---\nDo an audit."
        );
    }

    #[test]
    fn update_rejects_non_git_backed_plugin() {
        let install_root = tempfile::tempdir().unwrap();
        let plugin_dir = install_root.path().join("test-plugin");
        fs::create_dir_all(&plugin_dir).unwrap();
        fs::write(
            plugin_dir.join(INSTALL_METADATA),
            r#"{"source":"/tmp/test-plugin","source_type":"local","format":"gemini"}"#,
        )
        .unwrap();

        let err =
            update_plugin_at_root(Utc::now(), install_root.path(), "test-plugin").unwrap_err();

        assert!(err
            .to_string()
            .contains("cannot be updated with this command"));
    }

    fn write_gemini_plugin(repo: &Path, version: &str, description: &str) {
        fs::write(
            repo.join(formats::gemini::MANIFEST),
            format!(r#"{{"name":"test-plugin","version":"{version}"}}"#),
        )
        .unwrap();
        let skill_dir = repo.join("skills").join("audit");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: audit\ndescription: {description}\n---\nDo an audit."),
        )
        .unwrap();
    }

    fn init_git_repo(repo: &Path) {
        run_git(repo, &["init"]);
        run_git(repo, &["config", "user.email", "goose@example.com"]);
        run_git(repo, &["config", "user.name", "Goose"]);
    }

    fn commit_git_repo(repo: &Path, message: &str) {
        run_git(repo, &["add", "."]);
        run_git(repo, &["commit", "-m", message]);
    }

    fn run_git(repo: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo)
            .set_no_window()
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
