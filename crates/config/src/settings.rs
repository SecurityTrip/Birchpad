//! The settings schema. Every field has a default, so any layer may set any subset of keys,
//! and unknown keys are ignored so that settings written by a newer version still load.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Settings {
    pub editor: EditorSettings,
    pub files: FileSettings,
    pub session: SessionSettings,
    pub updates: UpdateSettings,
    pub plugins: PluginSettings,
    pub network: NetworkSettings,
    pub diagnostics: DiagnosticsSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct EditorSettings {
    pub tab_width: u8,
    pub insert_spaces: bool,
    pub word_wrap: bool,
    /// Margins left of the text, as in Notepad++'s Preferences > Margins: line numbers, the
    /// symbol margin (bookmarks) and the folding margin.
    pub line_numbers: bool,
    pub bookmark_margin: bool,
    pub fold_margin: bool,
    /// What Enter does with indentation.
    pub auto_indent: AutoIndent,
    /// The edge column: where Split Lines breaks lines (and, later, where the edge is drawn).
    pub edge_column: u16,
}

/// Notepad++'s auto-indent modes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AutoIndent {
    /// A new line starts at column 1.
    Off,
    /// A new line gets the indentation of the line above.
    Basic,
    /// Like basic, and one level more after an opening bracket (or a colon in Python); Enter
    /// between a pair of braces puts the closing one on its own line.
    #[default]
    Advanced,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            tab_width: 4,
            insert_spaces: false,
            word_wrap: false,
            line_numbers: true,
            bookmark_margin: true,
            fold_margin: true,
            auto_indent: AutoIndent::default(),
            edge_column: 80,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct FileSettings {
    /// How many recently closed files File > Recent Files remembers.
    pub recent_limit: u16,
    /// Legacy encoding for files that are neither Unicode nor recognizably something else
    /// ("ANSI"), e.g. `windows-1251`. Unset: the system code page on Windows, Windows-1252
    /// elsewhere.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ansi_encoding: Option<String>,
    /// Like Notepad++'s Large File Restriction: files larger than this many megabytes open
    /// without syntax highlighting, brace matching, smart highlighting and folding.
    pub large_file_limit_mb: u32,
}

impl Default for FileSettings {
    fn default() -> Self {
        Self {
            recent_limit: 10,
            ansi_encoding: None,
            large_file_limit_mb: 20,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct SessionSettings {
    /// Periodically back up unsaved changes so they survive closing the app or a crash.
    pub backup_unsaved: bool,
}

impl Default for SessionSettings {
    fn default() -> Self {
        Self {
            backup_unsaved: true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct UpdateSettings {
    pub mode: UpdateMode,
    pub channel: UpdateChannel,
    /// Update feed to use instead of the official one, e.g. an internal mirror.
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateMode {
    /// Never check for updates and never touch the network for them.
    Off,
    /// Check and tell the user, but do not download.
    Notify,
    /// Download in the background and install on restart.
    #[default]
    Auto,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateChannel {
    #[default]
    Stable,
    Beta,
    Nightly,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct PluginSettings {
    pub install: PluginInstall,
    /// Plugin ids that may be installed when `install` is `allowlist`.
    pub allowed: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginInstall {
    #[default]
    Allow,
    Allowlist,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct NetworkSettings {
    /// Features that talk to the network on their own (AI assistance, downloading language
    /// servers, the plugin catalog). Updates are controlled separately.
    pub online_features: bool,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            online_features: true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct DiagnosticsSettings {
    /// Send crash reports. Off unless the user opts in.
    pub crash_reports: bool,
}
