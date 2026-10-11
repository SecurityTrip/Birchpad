//! Application-wide state: resolved settings, persistent user state and where things are stored.

use std::path::{Path, PathBuf};

use birchpad_config::{ConfigPaths, ResolvedSettings, Settings, UserState};
use birchpad_core::Encoding;
use birchpad_io::LoadOptions;
use gpui_kit::{App, Global};

pub(crate) struct AppState {
    pub(crate) settings: Settings,
    pub(crate) paths: ConfigPaths,
    /// The legacy encoding used for "ANSI" files.
    pub(crate) ansi: Encoding,
    /// Recent files and other state kept between runs, in `state.toml`.
    pub(crate) state: UserState,
    /// Settings fixed by administrator policy, as `(key, value)`, for Help > About.
    pub(crate) policies: Vec<(String, String)>,
    /// `-nosession`: neither restore nor save the session.
    pub(crate) no_session: bool,
}

/// Where the session is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionPaths {
    pub(crate) file: PathBuf,
    /// The folder of the backup copies of unsaved text.
    pub(crate) backups: PathBuf,
}

impl Global for AppState {}

/// The legacy encoding `files.ansi-encoding` names, else the system's.
fn ansi_of(settings: &Settings) -> Encoding {
    let system = birchpad_io::system_ansi();
    match settings.files.ansi_encoding.as_deref() {
        None => system,
        Some(name) => match birchpad_io::parse_encoding(name, system) {
            Some((encoding, _)) if !encoding.is_unicode() => encoding,
            _ => {
                eprintln!("settings: files.ansi-encoding {name:?} is not a legacy encoding");
                system
            }
        },
    }
}

impl AppState {
    pub(crate) fn new(settings: ResolvedSettings, paths: ConfigPaths) -> Self {
        let policies = settings.policies();
        let settings = settings.settings;
        let ansi = ansi_of(&settings);
        let state = paths
            .user_data
            .as_ref()
            .map(|dir| UserState::load(&dir.join("state.toml")))
            .unwrap_or_default();
        Self {
            settings,
            paths,
            ansi,
            state,
            policies,
            no_session: false,
        }
    }

    /// Where the session lives, if the documents of this run are remembered for the next one
    /// (`session.remember`, not `-nosession`, and a data folder to keep it in).
    pub(crate) fn session_paths(&self) -> Option<SessionPaths> {
        if !self.settings.session.remember || self.no_session {
            return None;
        }
        let data = self.paths.user_data.as_ref()?;
        Some(SessionPaths {
            file: data.join("session.toml"),
            backups: data.join("backup"),
        })
    }

    /// Whether unsaved changes are kept in backup copies instead of asking about them on
    /// quitting (`session.backup-unsaved`, which needs a remembered session).
    pub(crate) fn backs_up_unsaved(&self) -> bool {
        self.settings.session.backup_unsaved && self.session_paths().is_some()
    }

    /// Takes settings read again, after Preferences changed one.
    pub(crate) fn set_settings(&mut self, settings: ResolvedSettings) {
        self.policies = settings.policies();
        self.settings = settings.settings;
        self.ansi = ansi_of(&self.settings);
    }

    /// Whether an administrator's policy sets `key`, which the user then cannot change.
    pub(crate) fn is_locked(&self, key: &str) -> bool {
        self.policies.iter().any(|(locked, _)| locked == key)
    }

    pub(crate) fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    /// Where saves keep the new content until the file is fully written.
    pub(crate) fn recovery_dir(&self) -> Option<PathBuf> {
        self.paths
            .user_data
            .as_ref()
            .map(|dir| dir.join("recovery"))
    }

    pub(crate) fn load_options(&self, encoding: Option<Encoding>) -> LoadOptions {
        LoadOptions {
            encoding,
            ansi: self.ansi,
        }
    }

    /// Changes the persistent state and writes it to disk.
    pub(crate) fn update_state(cx: &mut App, change: impl FnOnce(&mut UserState, &Settings)) {
        let this = cx.global_mut::<Self>();
        change(&mut this.state, &this.settings);
        if let Some(dir) = &this.paths.user_data
            && let Err(error) = this.state.save(&dir.join("state.toml"))
        {
            eprintln!("cannot save {}: {error}", dir.join("state.toml").display());
        }
    }

    /// Remembers a closed file in File > Recent Files.
    pub(crate) fn add_recent(path: &Path, cx: &mut App) {
        Self::update_state(cx, |state, settings| {
            state.add_recent(path, usize::from(settings.files.recent_limit));
        });
    }

    /// Forgets a file that is open again (open files are not "recent").
    pub(crate) fn remove_recent(path: &Path, cx: &mut App) {
        if Self::global(cx)
            .state
            .recent_files
            .iter()
            .any(|p| p == path)
        {
            Self::update_state(cx, |state, _| state.remove_recent(path));
        }
    }
}
