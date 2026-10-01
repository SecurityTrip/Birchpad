//! Application-wide state: resolved settings and where things are stored.

use std::path::PathBuf;

use birchpad_config::{ConfigPaths, ResolvedSettings};
use birchpad_core::Encoding;
use birchpad_io::LoadOptions;
use gpui_kit::{App, Global};

pub(crate) struct AppState {
    pub(crate) paths: ConfigPaths,
    /// The legacy encoding used for "ANSI" files.
    pub(crate) ansi: Encoding,
}

impl Global for AppState {}

impl AppState {
    pub(crate) fn new(settings: ResolvedSettings, paths: ConfigPaths) -> Self {
        let system = birchpad_io::system_ansi();
        let ansi = match settings.settings.files.ansi_encoding.as_deref() {
            None => system,
            Some(name) => match birchpad_io::parse_encoding(name, system) {
                Some((encoding, _)) if !encoding.is_unicode() => encoding,
                _ => {
                    eprintln!("settings: files.ansi-encoding {name:?} is not a legacy encoding");
                    system
                }
            },
        };
        Self { paths, ansi }
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
}
