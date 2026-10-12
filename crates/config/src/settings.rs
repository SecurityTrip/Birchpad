//! The settings schema. Every field has a default, so any layer may set any subset of keys,
//! and unknown keys are ignored so that settings written by a newer version still load.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Settings {
    pub appearance: AppearanceSettings,
    pub editor: EditorSettings,
    pub completion: CompletionSettings,
    pub highlighting: HighlightingSettings,
    pub files: FileSettings,
    pub tabs: TabSettings,
    pub session: SessionSettings,
    pub updates: UpdateSettings,
    pub plugins: PluginSettings,
    pub network: NetworkSettings,
    pub diagnostics: DiagnosticsSettings,
}

/// How Birchpad looks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct AppearanceSettings {
    /// The color theme as Birchpad starts: a built-in one (`Default`, `Dark`) or the name of a
    /// file in the `themes` folder next to the settings, without its extension. Settings > Theme
    /// chooses another (remembered in `state.toml`).
    pub theme: String,
    /// The interface's language: `en`, `ru`, or `auto` for the system's.
    pub language: String,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme: "Default".to_owned(),
            language: "auto".to_owned(),
        }
    }
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
    /// Lines changed since the document was opened, as Notepad++'s Change History shows them.
    pub change_history: ChangeHistory,
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

/// Where Notepad++'s change history shows changed lines: orange while they differ from the
/// saved file, green once saved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeHistory {
    Off,
    /// A bar in a margin between the symbols and the folding.
    #[default]
    Margin,
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
            change_history: ChangeHistory::default(),
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

/// Notepad++'s Preferences > Auto-Completion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct CompletionSettings {
    /// Brackets and quotes typed in pairs, written as pairs of characters: `()[]{}""''`. Typing the
    /// closing one steps over it; Backspace between an empty pair deletes both.
    pub auto_close: String,
    /// Show the words of the document and the language's API keywords while typing.
    pub words: bool,
    /// From how many characters of a word the list shows.
    pub min_chars: u8,
    /// Show a function's parameters after its opening bracket, from the language's API file.
    pub call_tips: bool,
}

impl Default for CompletionSettings {
    fn default() -> Self {
        Self {
            auto_close: "()[]{}\"\"''".to_owned(),
            words: true,
            min_chars: 1,
            call_tips: true,
        }
    }
}

impl CompletionSettings {
    /// The pairs of `auto_close`, two characters each; an odd character at the end is left out.
    pub fn pairs(&self) -> Vec<(char, char)> {
        let chars: Vec<char> = self.auto_close.chars().collect();
        let (pairs, _) = chars.as_chunks::<2>();
        pairs.iter().map(|[open, close]| (*open, *close)).collect()
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
    /// Notepad++'s File Status Auto-Detection: which open files are watched for changes made
    /// by other programs.
    pub change_detection: ChangeDetection,
    /// Reload a changed file that has no unsaved changes without asking ("Update silently").
    pub reload_silently: bool,
    /// After reloading, go to the end of the document ("Scroll to the last line after
    /// update").
    pub reload_scrolls_to_end: bool,
}

impl Default for FileSettings {
    fn default() -> Self {
        Self {
            recent_limit: 10,
            ansi_encoding: None,
            large_file_limit_mb: 20,
            change_detection: ChangeDetection::default(),
            reload_silently: false,
            reload_scrolls_to_end: false,
        }
    }
}

/// Which open files are watched for changes made by other programs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeDetection {
    /// Every open file.
    #[default]
    All,
    /// The document of the active tab only.
    Current,
    Off,
}

/// Notepad++'s Preferences > General > Tab Bar, and its Document Switcher (Ctrl+Tab).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct TabSettings {
    /// The tabs in a column on the left of each view ("Vertical").
    pub vertical: bool,
    /// Tabs that do not fit go on further rows instead of scrolling ("Multi-line").
    pub multi_line: bool,
    /// Smaller tabs ("Reduce").
    pub reduce: bool,
    /// Tabs cannot be dragged ("Lock (no drag and drop)").
    pub lock: bool,
    /// A close button on every tab ("Show close button on each tab").
    pub close_button: bool,
    /// A double click on a tab closes it ("Double click to close document").
    pub double_click_close: bool,
    /// Hovering over a tab that is not active shows the start of its text (the document
    /// peeker).
    pub peeker: bool,
    /// Ctrl+Tab shows the documents of the view in a list while Ctrl is held ("Document
    /// Switcher"); off, it goes to the next tab.
    pub switcher: bool,
    /// The switcher lists the documents most recently used first, instead of in tab order
    /// ("Enable MRU behaviour").
    pub switcher_mru: bool,
}

impl Default for TabSettings {
    fn default() -> Self {
        Self {
            vertical: false,
            multi_line: false,
            reduce: false,
            lock: false,
            close_button: true,
            double_click_close: false,
            peeker: true,
            switcher: true,
            switcher_mru: true,
        }
    }
}

/// Notepad++'s Preferences > Backup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct SessionSettings {
    /// Reopen the documents of the last run, with their carets, folds and bookmarks
    /// ("Remember current session for next launch").
    pub remember: bool,
    /// Keep unsaved changes, untitled documents included, in backup copies: quitting does not
    /// ask about them and the next launch restores them; a crash loses at most the last
    /// interval ("Enable session snapshot and periodic backup"). Needs `remember`.
    pub backup_unsaved: bool,
    /// How often unsaved changes are backed up, in seconds.
    pub backup_interval_seconds: u32,
}

impl Default for SessionSettings {
    fn default() -> Self {
        Self {
            remember: true,
            backup_unsaved: true,
            backup_interval_seconds: 7,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct UpdateSettings {
    pub mode: UpdateMode,
    pub channel: UpdateChannel,
    /// The signed update manifest to read instead of the official one, e.g. on an internal
    /// mirror; it must still be signed with a key Birchpad trusts.
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct PluginSettings {
    pub install: PluginInstall,
    /// Plugin ids that may be installed when `install` is `allowlist`.
    pub allowed: Vec<String>,
    /// Whether the scripts in the `scripts` folder may run.
    pub scripts: bool,
}

impl Default for PluginSettings {
    fn default() -> Self {
        Self {
            install: PluginInstall::default(),
            allowed: Vec::new(),
            scripts: true,
        }
    }
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

#[cfg(test)]
mod completion_tests {
    use super::*;

    #[test]
    fn pairs_of_auto_close() {
        let mut settings = CompletionSettings::default();
        assert_eq!(
            settings.pairs(),
            [('(', ')'), ('[', ']'), ('{', '}'), ('"', '"'), ('\'', '\'')]
        );
        settings.auto_close = "<>(".into();
        assert_eq!(settings.pairs(), [('<', '>')]);
        settings.auto_close.clear();
        assert!(settings.pairs().is_empty());
        settings.auto_close = "«»".into();
        assert_eq!(settings.pairs(), [('«', '»')]);
    }
}
