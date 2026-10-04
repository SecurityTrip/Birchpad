//! The settings schema. Every field has a default, so any layer may set any subset of keys,
//! and unknown keys are ignored so that settings written by a newer version still load.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Settings {
    pub editor: EditorSettings,
    pub highlighting: HighlightingSettings,
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
    /// View > Show Symbol as Birchpad starts; the menu toggles them (remembered in
    /// `state.toml`, like word wrap).
    pub show_whitespace: bool,
    pub show_eol: bool,
    pub indent_guides: bool,
    pub wrap_symbol: bool,
    /// How the line of the caret stands out.
    pub current_line: CurrentLine,
    /// Width of the frame around the current line in pixels (1 to 6), as in Notepad++.
    pub current_line_frame_width: u8,
    /// The vertical edge, as in Notepad++'s Preferences > Margins/Border/Edge.
    pub edge: Edge,
    /// Columns of the edge: a line at each in line mode, the first one in background mode.
    /// Split Lines breaks lines at the first one while the edge is shown, else at the width of
    /// the view.
    pub edge_columns: Vec<u16>,
}

/// Notepad++'s current line indicator.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CurrentLine {
    Off,
    /// A background across the whole line.
    #[default]
    Background,
    /// A frame around the line.
    Frame,
}

/// Notepad++'s vertical edge modes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Edge {
    #[default]
    Off,
    /// A vertical line at each edge column.
    Line,
    /// Text past the first edge column gets the edge color as its background.
    Background,
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
            show_whitespace: false,
            show_eol: false,
            indent_guides: true,
            wrap_symbol: false,
            current_line: CurrentLine::default(),
            current_line_frame_width: 1,
            edge: Edge::default(),
            edge_columns: vec![80],
        }
    }
}

/// Notepad++'s Preferences > Highlighting.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct HighlightingSettings {
    /// Highlighting every occurrence of the selected word in the visible text.
    pub smart: SmartHighlighting,
    /// How Search > Style All Occurrences of Token matches the token.
    pub token_style: TokenMatching,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct SmartHighlighting {
    pub enabled: bool,
    pub match_case: bool,
    /// Highlight only when a whole word is selected, and only whole-word occurrences of it.
    /// Off: any selection on one line is highlighted wherever it occurs.
    pub whole_word: bool,
    /// Take match case and whole word from the find panel instead.
    pub use_find_options: bool,
}

impl Default for SmartHighlighting {
    fn default() -> Self {
        Self {
            enabled: true,
            match_case: false,
            whole_word: true,
            use_find_options: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct TokenMatching {
    pub match_case: bool,
    pub whole_word: bool,
}

impl Default for TokenMatching {
    fn default() -> Self {
        Self {
            match_case: false,
            whole_word: true,
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
