//! Application state that is not a setting: recently opened files, the zoom level. It lives in
//! `state.toml` in the user data directory, separate from `settings.toml`, so that the settings
//! file only changes when the user changes a setting.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The most recent files any setting can ask to remember.
pub const MAX_RECENT_FILES: usize = 100;
/// The most commands the command palette remembers as recently used.
pub const MAX_RECENT_COMMANDS: usize = 50;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct UserState {
    /// Recently closed files, most recent first (as in Notepad++, files that are open are not
    /// listed).
    pub recent_files: Vec<PathBuf>,
    /// Zoom level in steps relative to the default font size.
    pub zoom: i32,
    /// View > Word Wrap as last toggled; unset means `editor.word-wrap` from the settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub word_wrap: Option<bool>,
    /// View > Show Symbol as last toggled; unset means the `editor.*` setting of the same name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_whitespace: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_eol: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indent_guides: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrap_symbol: Option<bool>,
    /// Settings > Theme as last chosen; unset means `appearance.theme` from the settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// How split view places the two views, as last rotated.
    pub split: SplitOrientation,
    /// What update checks remember.
    pub updates: UpdateState,
    /// The fields and options of Find in Files as last used.
    pub find_in_files: FindInFilesState,
    /// The side panels.
    pub panels: PanelsState,
    /// Plugins > Spell Checker.
    pub spell_check: SpellCheckState,
    /// The search results' options.
    pub search_results: SearchResultsState,
    /// How a document with a preview (Markdown) is shown, as last chosen.
    pub preview: PreviewMode,
    /// The folder last chosen in an Open or Save As dialog (Default Directory > Remember).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_directory: Option<PathBuf>,
    /// The offer to open text files with Birchpad was answered "Don't Ask Again".
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub default_app_declined: bool,
    /// The commands last run from the command palette, most recent first: each an invocation
    /// as JSON (`{"command":"file.save"}`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub recent_commands: Vec<String>,
}

/// The search results' context menu switches.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct SearchResultsState {
    /// Long lines wrap instead of being cut at the panel's edge.
    pub wrap_long_lines: bool,
    /// Each search clears the results of the earlier ones.
    pub purge: bool,
}

/// What the spell checker remembers between runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct SpellCheckState {
    /// Spell Check Document Automatically: misspelled words underlined.
    pub enabled: bool,
    /// The dictionary's language (`en_US`); unset means the system's language.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

/// What the side panels remember between runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct PanelsState {
    /// The panels open when Birchpad last quit, in the order they were opened:
    /// `function-list`, `project-1`, ...
    pub open: Vec<String>,
    /// The top folders of Folder as Workspace.
    pub folders: Vec<PathBuf>,
    /// The workspace file of each project panel, by its number ("1" to "3").
    pub projects: std::collections::BTreeMap<String, PathBuf>,
}

/// What the Find in Files tab remembers between runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct FindInFilesState {
    /// Notepad++'s filters: `*.rs *.toml !\target`.
    pub filters: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directory: Option<PathBuf>,
    /// In all sub-folders.
    pub subfolders: bool,
    /// In hidden folders.
    pub hidden: bool,
    /// Follow current doc.: the folder of the active document whenever the tab opens.
    pub follow_current_document: bool,
}

impl Default for FindInFilesState {
    fn default() -> Self {
        Self {
            filters: String::new(),
            directory: None,
            // Notepad++'s defaults.
            subfolders: true,
            hidden: false,
            follow_current_document: false,
        }
    }
}

/// What update checks remember between runs (ADR 0020).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct UpdateState {
    /// When a check last succeeded, in seconds since 1970.
    pub last_check: u64,
    /// When the newest update manifest seen was signed: older ones are refused, so that an update
    /// server cannot go back to an old manifest.
    pub manifest_timestamp: u64,
    /// The newest version announced by a background check, which is announced only once.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub announced: Option<String>,
}

/// How a document with a preview is shown, as IntelliJ IDEA shows Markdown: the text, the text
/// and its preview side by side, or the preview alone.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PreviewMode {
    Editor,
    #[default]
    Split,
    Preview,
}

/// How split view places its two views.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SplitOrientation {
    /// Side by side, Notepad++'s default.
    #[default]
    SideBySide,
    /// One above the other.
    Stacked,
}

impl SplitOrientation {
    pub fn rotated(self) -> Self {
        match self {
            Self::SideBySide => Self::Stacked,
            Self::Stacked => Self::SideBySide,
        }
    }
}

impl UserState {
    /// Reads the state file. A missing or damaged file gives the default state: losing the
    /// recent files list is better than refusing to start.
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// The state in the text of a state file, `None` if it is damaged.
    pub fn parse(text: &str) -> Option<Self> {
        toml::from_str(text).ok()
    }

    /// Writes the state file, replacing it atomically.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let text = toml::to_string(self).map_err(io::Error::other)?;
        let temporary = path.with_extension("toml.tmp");
        fs::write(&temporary, text)?;
        fs::rename(&temporary, path)
    }

    /// Puts `path` at the top of the recent files, keeping at most `limit` entries.
    pub fn add_recent(&mut self, path: &Path, limit: usize) {
        self.remove_recent(path);
        self.recent_files.insert(0, path.to_owned());
        self.recent_files.truncate(limit.min(MAX_RECENT_FILES));
    }

    pub fn remove_recent(&mut self, path: &Path) {
        self.recent_files.retain(|existing| existing != path);
    }

    /// Puts the command `key` at the top of the recently used commands, keeping at most
    /// `limit`; a limit of 0 forgets them all.
    pub fn add_recent_command(&mut self, key: &str, limit: usize) {
        self.recent_commands.retain(|existing| existing != key);
        self.recent_commands.insert(0, key.to_owned());
        self.recent_commands
            .truncate(limit.min(MAX_RECENT_COMMANDS));
    }
}
