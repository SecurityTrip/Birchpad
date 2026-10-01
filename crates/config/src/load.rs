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
    /// Portable mode: user settings and data live next to the executable.
    pub portable: bool,
}

/// A file with this name next to the executable turns on portable mode.
pub const PORTABLE_MARKER: &str = "birchpad-portable.txt";
/// In portable mode, the directory next to the executable that holds settings and data.
pub const PORTABLE_DATA_DIR: &str = "data";

impl ConfigPaths {
    /// The directory of the user's settings file, where `keymap.toml` lives too.
    pub fn user_config_dir(&self) -> Option<&Path> {
        self.user_settings.as_deref().and_then(Path::parent)
    }

    /// The locations for the executable at `exe`: portable if [`PORTABLE_MARKER`] is next to
    /// it, the platform's conventional ones otherwise.
    pub fn for_executable(exe: &Path) -> Self {
        match exe.parent() {
            Some(dir) if dir.join(PORTABLE_MARKER).is_file() => Self::portable(dir),
            _ => Self::platform(),
        }
    }

    /// The locations for the running executable (see [`Self::for_executable`]).
    pub fn current() -> Self {
        match env::current_exe() {
            Ok(exe) => Self::for_executable(&exe),
            Err(_) => Self::platform(),
        }
    }

    /// Portable mode for an executable in `dir`: the user's settings, keymap, recent files and
    /// recovery copies go to `dir/data`. Machine defaults and administrator policies still come
    /// from the system, so a portable copy cannot escape the policies of the machine it runs on.
    pub fn portable(dir: &Path) -> Self {
        let data = dir.join(PORTABLE_DATA_DIR);
        Self {
            user_settings: Some(data.join("settings.toml")),
            user_data: Some(data),
            portable: true,
            ..Self::platform()
        }
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
                portable: false,
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
                portable: false,
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
                portable: false,
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
    resolve(load(&ConfigPaths::current()))
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
            portable: false,
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

    #[test]
    fn a_marker_next_to_the_executable_makes_it_portable() {
        let dir = env::temp_dir().join(format!("birchpad-portable-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("birchpad.exe");
        let installed = ConfigPaths::for_executable(&exe);
        fs::write(dir.join(PORTABLE_MARKER), "").unwrap();
        let portable = ConfigPaths::for_executable(&exe);
        fs::remove_dir_all(&dir).unwrap();

        assert_eq!(installed, ConfigPaths::platform());
        assert!(portable.portable);
        assert_eq!(portable.user_data, Some(dir.join("data")));
        assert_eq!(portable.user_config_dir(), Some(dir.join("data").as_path()));
        // Policies and machine defaults are not the portable copy's to choose.
        assert_eq!(portable.policy_file, installed.policy_file);
        assert_eq!(portable.machine_defaults, installed.machine_defaults);
    }
}
