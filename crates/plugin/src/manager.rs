//! The plugins folder: which plugins are installed, which are turned off, and installing and
//! removing them under the administrator's policy (ADR 0038).
//!
//! Each plugin lives in `plugins/<id>/` with its `plugin.toml` and component;
//! `plugins/disabled.toml` lists the plugins turned off.

use birchpad_i18n::t;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow, bail};
use birchpad_config::{PluginInstall, PluginSettings};
use serde::{Deserialize, Serialize};

use crate::manifest::Manifest;

/// The file that describes a plugin.
pub const MANIFEST: &str = "plugin.toml";
/// The file listing the plugins turned off, in the plugins folder.
pub const DISABLED: &str = "disabled.toml";
/// The most bytes a plugin's folder may hold when installed.
pub const MAX_PLUGIN_BYTES: u64 = 64 << 20;

/// An installed plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub manifest: Manifest,
    pub dir: PathBuf,
}

impl Installed {
    /// The component's file.
    pub fn entry(&self) -> PathBuf {
        self.dir.join(&self.manifest.entry)
    }
}

/// What is in the plugins folder.
#[derive(Debug, Default)]
pub struct Discovery {
    /// Readable plugins, by name.
    pub plugins: Vec<Installed>,
    /// Folders that hold no readable plugin, with why.
    pub problems: Vec<String>,
}

/// Reads every plugin in `dir`. A missing folder holds no plugins.
pub fn discover(dir: &Path) -> Discovery {
    let mut found = Discovery::default();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    for plugin_dir in dirs {
        match read_plugin(&plugin_dir) {
            Ok(installed) => {
                if installed.manifest.id != folder_name(&plugin_dir) {
                    found.problems.push(format!(
                        "{}: the folder of the plugin {} must be named after it",
                        plugin_dir.display(),
                        installed.manifest.id
                    ));
                } else {
                    found.plugins.push(installed);
                }
            }
            Err(error) => found
                .problems
                .push(format!("{}: {error:#}", plugin_dir.display())),
        }
    }
    found
        .plugins
        .sort_by_key(|plugin| plugin.manifest.name.to_lowercase());
    found
}

fn folder_name(dir: &Path) -> String {
    dir.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The plugin in `dir`.
pub fn read_plugin(dir: &Path) -> Result<Installed> {
    let path = dir.join(MANIFEST);
    let text = std::fs::read_to_string(&path)
        .with_context(|| t!("plugin.cannot-read", path = path.display()))?;
    let manifest = Manifest::parse(&text)?;
    let installed = Installed {
        manifest,
        dir: dir.to_owned(),
    };
    if !installed.entry().is_file() {
        bail!(
            "{}",
            t!("plugin.no-entry", path = installed.entry().display())
        );
    }
    Ok(installed)
}

/// Whether the administrator's policy lets the plugin `id` be installed and run.
pub fn permitted(id: &str, settings: &PluginSettings) -> Result<(), String> {
    match settings.install {
        PluginInstall::Allow => Ok(()),
        PluginInstall::Deny => Err(t!("plugin.turned-off")),
        PluginInstall::Allowlist if settings.allowed.iter().any(|allowed| allowed == id) => Ok(()),
        PluginInstall::Allowlist => Err(t!("plugin.not-allowed", id = id)),
    }
}

/// Installs the plugin whose `plugin.toml` is `manifest_path` into `plugins_dir`, copying
/// its folder; an installed plugin of the same id is replaced (an update).
pub fn install(
    manifest_path: &Path,
    plugins_dir: &Path,
    settings: &PluginSettings,
) -> Result<Installed> {
    let source = manifest_path.parent().ok_or_else(|| {
        anyhow!(
            "{}",
            t!("plugin.not-in-a-folder", path = manifest_path.display())
        )
    })?;
    let plugin = read_plugin(source)?;
    let id = plugin.manifest.id.clone();
    permitted(&id, settings).map_err(|message| anyhow!(message))?;
    let target = plugins_dir.join(&id);
    let same = |a: &Path, b: &Path| match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    };
    if same(source, &target) {
        bail!("{}", t!("plugin.already-installed", id = id));
    }
    let size = folder_size(source)?;
    if size > MAX_PLUGIN_BYTES {
        bail!(
            "{}",
            t!(
                "plugin.too-large",
                id = id,
                size = size,
                limit = MAX_PLUGIN_BYTES
            )
        );
    }
    // Copy next to the target first, so that a failed copy leaves the old version in place.
    let staging = plugins_dir.join(format!(".{id}.installing"));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(plugins_dir)
        .with_context(|| t!("plugin.cannot-create", path = plugins_dir.display()))?;
    if let Err(error) = copy_folder(source, &staging) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error);
    }
    if target.exists() {
        std::fs::remove_dir_all(&target)
            .with_context(|| t!("plugin.cannot-remove-old", path = target.display()))?;
    }
    std::fs::rename(&staging, &target)
        .with_context(|| t!("plugin.cannot-move", path = target.display()))?;
    read_plugin(&target)
}

fn folder_size(dir: &Path) -> Result<u64> {
    let mut total = 0;
    for entry in
        std::fs::read_dir(dir).with_context(|| t!("plugin.cannot-read", path = dir.display()))?
    {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            total += folder_size(&entry.path())?;
        } else if kind.is_file() {
            total += entry.metadata()?.len();
        }
    }
    Ok(total)
}

/// Copies the files and folders of `from` into `to`; links are left out.
fn copy_folder(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to).with_context(|| t!("plugin.cannot-create", path = to.display()))?;
    for entry in
        std::fs::read_dir(from).with_context(|| t!("plugin.cannot-read", path = from.display()))?
    {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_folder(&entry.path(), &target)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), &target)
                .with_context(|| t!("plugin.cannot-copy", path = entry.path().display()))?;
        }
    }
    Ok(())
}

/// Removes the plugin `id` from `plugins_dir`.
pub fn uninstall(plugins_dir: &Path, id: &str) -> Result<()> {
    if !crate::manifest::is_valid_id(id) {
        bail!("{}", t!("plugin.not-an-id", id = format!("{id:?}")));
    }
    let dir = plugins_dir.join(id);
    if !dir.join(MANIFEST).is_file() {
        bail!("{}", t!("plugin.not-installed", id = id));
    }
    std::fs::remove_dir_all(&dir).with_context(|| t!("plugin.cannot-remove", path = dir.display()))
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct DisabledFile {
    #[serde(default)]
    disabled: Vec<String>,
}

/// The plugins turned off. A file that does not read turns none off.
pub fn read_disabled(plugins_dir: &Path) -> Vec<String> {
    std::fs::read_to_string(plugins_dir.join(DISABLED))
        .ok()
        .and_then(|text| toml::from_str::<DisabledFile>(&text).ok())
        .map(|file| file.disabled)
        .unwrap_or_default()
}

/// Turns the plugin `id` on or off.
pub fn set_enabled(plugins_dir: &Path, id: &str, enabled: bool) -> Result<()> {
    let mut disabled = read_disabled(plugins_dir);
    disabled.retain(|known| known != id);
    if !enabled {
        disabled.push(id.to_owned());
    }
    disabled.sort();
    std::fs::create_dir_all(plugins_dir)
        .with_context(|| t!("plugin.cannot-create", path = plugins_dir.display()))?;
    let path = plugins_dir.join(DISABLED);
    let text = toml::to_string(&DisabledFile { disabled }).unwrap_or_default();
    std::fs::write(&path, text).with_context(|| t!("plugin.cannot-write", path = path.display()))
}
