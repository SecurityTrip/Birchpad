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
    /// With a cloud folder (`cloud.folder`), the local settings file that names it;
    /// `user_settings` is then the folder's.
    pub local_settings: Option<PathBuf>,
}

/// A file with this name next to the executable turns on portable mode.
pub const PORTABLE_MARKER: &str = "birchpad-portable.txt";
/// In portable mode, the directory next to the executable that holds settings and data.
pub const PORTABLE_DATA_DIR: &str = "data";

impl ConfigPaths {
    /// The paths with the user's settings in the cloud folder their local file names, if it
    /// names one that exists; otherwise as they are.
    pub fn following_cloud(self) -> Self {
        let Some(local) = self.user_settings.clone() else {
            return self;
        };
        let folder = fs::read_to_string(&local)
            .ok()
            .and_then(|text| toml::from_str::<Table>(&text).ok())
            .and_then(|table| {
                table
                    .get("cloud")?
                    .get("folder")?
                    .as_str()
                    .map(|folder| folder.trim().to_owned())
            })
            .filter(|folder| !folder.is_empty())
            .map(PathBuf::from)
            .filter(|folder| folder.is_dir());
        match folder {
            Some(folder) => Self {
                user_settings: Some(folder.join("settings.toml")),
                local_settings: Some(local),
                ..self
            },
            None => self,
        }
    }

    /// The file a setting is written to: `cloud.folder` stays in the local one.
    pub fn settings_file_for(&self, key: &str) -> Option<&Path> {
        if key.starts_with("cloud.") {
            self.local_settings
                .as_deref()
                .or(self.user_settings.as_deref())
        } else {
            self.user_settings.as_deref()
        }
    }

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
                local_settings: None,
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
                local_settings: None,
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
                local_settings: None,
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
    let mut user = read(Layer::User, &paths.user_settings);
    // With a cloud folder, `cloud` itself comes from the local file.
    if let Some(local) = read(Layer::User, &paths.local_settings)
        && let Some(cloud) = local.get("cloud").cloned()
    {
        user.get_or_insert_with(Table::new)
            .insert("cloud".to_owned(), cloud);
    }
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
            local_settings: None,
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
    fn unreadable_files_are_reported_and_empty_ones_are_empty_layers() {
        let dir = tempfile::tempdir().unwrap();
        // A folder where the policy file should be cannot be read; an empty file is a layer
        // with nothing in it; a good machine file is read.
        let policy = dir.path().join("policies.toml");
        fs::create_dir(&policy).unwrap();
        let user = dir.path().join("settings.toml");
        fs::write(&user, "").unwrap();
        let machine = dir.path().join("defaults.toml");
        fs::write(&machine, "editor.tab-width = 2").unwrap();

        let sources = load(&ConfigPaths {
            machine_defaults: Some(machine),
            user_settings: Some(user),
            policy_file: Some(policy),
            user_data: None,
            portable: false,
            local_settings: None,
        });
        assert_eq!(
            sources.machine.and_then(|t| t.get("editor").cloned()),
            Some(toml::Value::Table(toml::from_str("tab-width = 2").unwrap()))
        );
        assert_eq!(sources.user, Some(Table::new()));
        let policy_problems = sources
            .diagnostics
            .iter()
            .filter(|d| d.layer == Layer::Policy && d.key.is_none())
            .count();
        assert_eq!(policy_problems, 1, "{:?}", sources.diagnostics);
    }

    #[test]
    fn no_paths_load_nothing() {
        let sources = load(&ConfigPaths {
            machine_defaults: None,
            user_settings: None,
            policy_file: None,
            user_data: None,
            portable: false,
            local_settings: None,
        });
        assert!(sources.machine.is_none() && sources.user.is_none());
        assert_eq!(ConfigPaths::default().user_config_dir(), None);
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

    #[test]
    fn settings_in_a_cloud_folder() {
        let dir = tempfile::tempdir().unwrap();
        let local = dir.path().join("settings.toml");
        let cloud = dir.path().join("cloud");
        fs::create_dir(&cloud).unwrap();
        fs::write(cloud.join("settings.toml"), "editor.tab-width = 3").unwrap();
        let paths = ConfigPaths {
            user_settings: Some(local.clone()),
            ..ConfigPaths::default()
        };
        // No local file, or no folder in it, or a folder that is not there: as they are.
        assert_eq!(paths.clone().following_cloud(), paths);
        fs::write(&local, "cloud.folder = \"\"").unwrap();
        assert_eq!(paths.clone().following_cloud(), paths);
        fs::write(&local, "cloud.folder = \"/no/such/folder\"").unwrap();
        assert_eq!(paths.clone().following_cloud(), paths);
        fs::write(&local, "[cloud\n").unwrap();
        assert_eq!(paths.clone().following_cloud(), paths);

        let text = format!("cloud.folder = '{}'\neditor.tab-width = 9", cloud.display());
        fs::write(&local, text).unwrap();
        let following = paths.following_cloud();
        assert_eq!(following.user_settings, Some(cloud.join("settings.toml")));
        assert_eq!(following.local_settings, Some(local.clone()));
        assert_eq!(
            following.settings_file_for("cloud.folder"),
            Some(local.as_path())
        );
        assert_eq!(
            following.settings_file_for("editor.tab-width"),
            Some(cloud.join("settings.toml").as_path())
        );
        let resolved = resolve(load(&following));
        assert_eq!(
            resolved.settings.editor.tab_width, 3,
            "the cloud's settings"
        );
        assert_eq!(resolved.settings.cloud.folder, cloud.display().to_string());
    }
}
