//! Locating and reading configuration files.

use std::path::{Path, PathBuf};
use std::{env, fs, io};

use toml::Table;

use crate::layers::{Diagnostic, Layer, ResolvedSettings, Sources, resolve};

/// Where each configuration layer lives on this machine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigPaths {
    /// Machine-wide defaults written by an administrator or the installer.
    pub machine_defaults: Option<PathBuf>,
    /// The user's settings.
    pub user_settings: Option<PathBuf>,
    /// Policy file, on platforms without a policy registry.
    pub policy_file: Option<PathBuf>,
    /// Per-user application data that is not a setting: recent files, recovery copies, and
    /// later sessions and backups.
    pub user_data: Option<PathBuf>,
}

impl ConfigPaths {
    /// The directory of the user's settings file, where `keymap.toml` lives too.
    pub fn user_config_dir(&self) -> Option<&Path> {
        self.user_settings.as_deref().and_then(Path::parent)
    }
}

impl ConfigPaths {
    /// The conventional locations for the current platform:
    ///
    /// | | Windows | macOS | Linux |
    /// |---|---|---|---|
    /// | machine | `%ProgramData%\Birchpad\defaults.toml` | `/Library/Application Support/Birchpad/defaults.toml` | `/etc/birchpad/defaults.toml` |
    /// | user | `%APPDATA%\Birchpad\settings.toml` | `~/Library/Application Support/Birchpad/settings.toml` | `$XDG_CONFIG_HOME/birchpad/settings.toml` |
    /// | policy | registry | `/Library/Application Support/Birchpad/policies.toml` | `/etc/birchpad/policies.toml` |
    /// | data | `%LOCALAPPDATA%\Birchpad` | `~/Library/Application Support/Birchpad` | `$XDG_DATA_HOME/birchpad` |
    pub fn platform() -> Self {
        if cfg!(windows) {
            let program_data =
                env_path("ProgramData").unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
            Self {
                machine_defaults: Some(program_data.join("Birchpad").join("defaults.toml")),
                user_settings: env_path("APPDATA")
                    .map(|dir| dir.join("Birchpad").join("settings.toml")),
                policy_file: None,
                user_data: env_path("LOCALAPPDATA").map(|dir| dir.join("Birchpad")),
            }
        } else if cfg!(target_os = "macos") {
            let system = Path::new("/Library/Application Support/Birchpad");
            let user =
                env_path("HOME").map(|home| home.join("Library/Application Support/Birchpad"));
            Self {
                machine_defaults: Some(system.join("defaults.toml")),
                user_settings: user.as_ref().map(|dir| dir.join("settings.toml")),
                policy_file: Some(system.join("policies.toml")),
                user_data: user,
            }
        } else {
            let config_home = env_path("XDG_CONFIG_HOME")
                .or_else(|| env_path("HOME").map(|home| home.join(".config")));
            let data_home = env_path("XDG_DATA_HOME")
                .or_else(|| env_path("HOME").map(|home| home.join(".local/share")));
            Self {
                machine_defaults: Some(PathBuf::from("/etc/birchpad/defaults.toml")),
                user_settings: config_home.map(|dir| dir.join("birchpad").join("settings.toml")),
                policy_file: Some(PathBuf::from("/etc/birchpad/policies.toml")),
                user_data: data_home.map(|dir| dir.join("birchpad")),
            }
        }
    }
}

/// Reads every layer. Missing files are fine; unreadable or malformed ones are reported.
pub fn load(paths: &ConfigPaths) -> Sources {
    let mut diagnostics = Vec::new();
    let mut read = |layer, path: &Option<PathBuf>| {
        path.as_deref()
            .and_then(|path| read_table(layer, path, &mut diagnostics))
    };
    let machine = read(Layer::Machine, &paths.machine_defaults);
    let user = read(Layer::User, &paths.user_settings);
    let policy = read(Layer::Policy, &paths.policy_file);

    #[cfg(windows)]
    let policy = {
        let (registry, registry_diagnostics) = crate::policy::registry::read();
        diagnostics.extend(registry_diagnostics);
        if registry.is_empty() {
            policy
        } else {
            Some(registry)
        }
    };

    Sources {
        machine,
        user,
        policy,
        diagnostics,
    }
}

/// Reads and resolves settings from the conventional locations.
pub fn load_platform_settings() -> ResolvedSettings {
    resolve(load(&ConfigPaths::platform()))
}

fn read_table(layer: Layer, path: &Path, diagnostics: &mut Vec<Diagnostic>) -> Option<Table> {
    let report = |message: String| Diagnostic {
        layer,
        key: None,
        message,
    };
    match fs::read_to_string(path) {
        Ok(text) => match toml::from_str(&text) {
            Ok(table) => Some(table),
            Err(error) => {
                diagnostics.push(report(format!("{}: {error}", path.display())));
                None
            }
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => {
            diagnostics.push(report(format!("{}: {error}", path.display())));
            None
        }
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_files_are_not_errors_but_broken_ones_are() {
        let dir = env::temp_dir().join(format!("birchpad-config-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let broken = dir.join("settings.toml");
        fs::write(&broken, "editor = [unclosed").unwrap();

        let sources = load(&ConfigPaths {
            machine_defaults: Some(dir.join("missing.toml")),
            user_settings: Some(broken),
            policy_file: None,
            user_data: None,
        });
        fs::remove_dir_all(&dir).unwrap();

        assert!(sources.machine.is_none());
        assert!(sources.user.is_none());
        let user_problems: Vec<_> = sources
            .diagnostics
            .iter()
            .filter(|d| d.layer == Layer::User)
            .collect();
        assert_eq!(user_problems.len(), 1);
    }
}
