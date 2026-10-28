//! Preferences > Cloud & Link.

use serde::{Deserialize, Serialize};

/// Clickable links: Notepad++'s Preferences > Cloud & Link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct LinkSettings {
    /// Addresses can be opened with Ctrl+click (Cmd+click on macOS).
    pub enabled: bool,
    /// A double click opens them too, as in Notepad++; off, it selects a word of them, as
    /// elsewhere (issue #45).
    pub double_click: bool,
    /// They are underlined.
    pub underline: bool,
    /// Schemes besides http, https, ftp, ftps, file and mailto, separated by spaces
    /// ("URI customized schemes").
    pub schemes: String,
    /// A link under the mouse is framed, not only colored ("Full box mode").
    pub hover_box: bool,
}

impl Default for LinkSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            double_click: false,
            underline: true,
            schemes: "svn:// git:// ssh:// sftp://".to_owned(),
            hover_box: false,
        }
    }
}

impl LinkSettings {
    /// The schemes of one's own.
    pub fn extra_schemes(&self) -> Vec<String> {
        self.schemes.split_whitespace().map(str::to_owned).collect()
    }
}

/// Settings kept in a folder of one's choice (Notepad++'s Preferences > Cloud & Link).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct CloudSettings {
    /// The folder whose `settings.toml` (and `keymap.toml`, themes...) Birchpad uses from the
    /// next start; empty: the usual place. Only the local settings file can set it.
    pub folder: String,
}
