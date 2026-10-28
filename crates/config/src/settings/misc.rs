//! Preferences > MISC.

use serde::{Deserialize, Serialize};

/// Notepad++'s Preferences > MISC. that Birchpad has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct MiscSettings {
    /// The document peeker shows an inactive tab in the Document Map, when it is open, rather
    /// than in a popup.
    pub peek_on_document_map: bool,
    /// Files without a byte order mark are examined for their encoding; off, they open in
    /// UTF-8, or ANSI if they are not ("Autodetect character encoding").
    pub detect_encoding: bool,
    /// Save All asks first ("Enable Save All confirm dialog").
    pub confirm_save_all: bool,
    /// Folder as Workspace follows links to folders.
    pub follow_symlinks: bool,
    /// Minimizing the window takes it off the taskbar to an icon in the notification area (the
    /// menu bar on macOS), which brings it back ("Minimize to system tray"). Windows and macOS
    /// only.
    pub minimize_to_tray: bool,
}

impl Default for MiscSettings {
    fn default() -> Self {
        Self {
            peek_on_document_map: false,
            detect_encoding: true,
            confirm_save_all: false,
            follow_symlinks: false,
            minimize_to_tray: false,
        }
    }
}
