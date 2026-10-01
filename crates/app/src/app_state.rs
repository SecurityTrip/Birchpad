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
}

impl Global for AppState {}

impl AppState {
    pub(crate) fn new(settings: ResolvedSettings, paths: ConfigPaths) -> Self {
        let settings = settings.settings;
        let system = birchpad_io::system_ansi();
        let ansi = match settings.files.ansi_encoding.as_deref() {
            None => system,
            Some(name) => match birchpad_io::parse_encoding(name, system) {
                Some((encoding, _)) if !encoding.is_unicode() => encoding,
                _ => {
                    eprintln!("settings: files.ansi-encoding {name:?} is not a legacy encoding");
                    system
                }
            },
        };
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
        }
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
