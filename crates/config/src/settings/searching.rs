//! Preferences > Searching and Search Engine, Go to File and the command palette.

use serde::{Deserialize, Serialize};

/// Notepad++'s Preferences > Searching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct SearchingSettings {
    /// A selection of at least this many characters turns In selection on when the find
    /// panel opens; 0 never does.
    pub in_selection_from: u32,
    /// The selection, up to this many characters, fills the find field when the panel opens;
    /// 0 never fills it.
    pub fill_from_selection: u32,
    /// With nothing selected, the word at the caret fills it.
    pub fill_from_word: bool,
    /// The find fields in a monospaced font.
    pub monospaced: bool,
    /// The find panel stays open after a search that lists its results.
    pub stay_open: bool,
    /// Replace All in All Opened Documents asks first.
    pub confirm_replace_all_documents: bool,
    /// Replace stays on what it replaced instead of going to the next match.
    pub replace_stays: bool,
    /// The search results list each line once, whatever it holds; off, once for each match.
    pub one_entry_per_line: bool,
    /// Find in Files searches the files on disk, not what open documents hold unsaved.
    pub ignore_unsaved_changes: bool,
    /// Find in Files' folder is the active document's when the panel opens.
    pub folder_from_document: bool,
}

impl Default for SearchingSettings {
    fn default() -> Self {
        Self {
            in_selection_from: 1024,
            fill_from_selection: 1024,
            fill_from_word: true,
            monospaced: false,
            stay_open: true,
            confirm_replace_all_documents: true,
            replace_stays: false,
            one_entry_per_line: true,
            ignore_unsaved_changes: false,
            folder_from_document: false,
        }
    }
}

/// Search > Go to File...
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct FinderSettings {
    /// Which files of the workspace's folders are listed, written as Find in Files' filters:
    /// `!+\target` leaves out every folder `target`, `!*.bak` the backups.
    pub filters: String,
    /// Hidden files and folders are listed too.
    pub hidden: bool,
    /// The most files listed, from 1000 to 1000000: the rest of a large folder is left out.
    pub max_files: u32,
}

impl Default for FinderSettings {
    fn default() -> Self {
        Self {
            filters: r"!+\.git !+\node_modules !+\target".to_owned(),
            hidden: false,
            max_files: 50_000,
        }
    }
}

impl FinderSettings {
    /// [`Self::max_files`] within its limits.
    pub fn file_limit(&self) -> usize {
        self.max_files.clamp(1000, 1_000_000) as usize
    }
}

/// View > Command Palette...
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct PaletteSettings {
    /// How many of the commands last run from the palette it lists first, from 0 (none, and
    /// none remembered) to 50.
    pub recent_commands: u8,
}

impl Default for PaletteSettings {
    fn default() -> Self {
        Self { recent_commands: 8 }
    }
}

impl PaletteSettings {
    /// [`Self::recent_commands`] within its limit.
    pub fn recent_limit(&self) -> usize {
        usize::from(self.recent_commands).min(crate::state::MAX_RECENT_COMMANDS)
    }
}

/// Edit > On Selection > Search on Internet: Notepad++'s Preferences > Search Engine.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct SearchEngineSettings {
    pub engine: SearchEngine,
    /// With `engine = "custom"`: the address, `$(CURRENT_WORD)` standing for what is searched
    /// (`https://example.com/search?q=$(CURRENT_WORD)`).
    pub custom_url: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SearchEngine {
    #[default]
    Duckduckgo,
    Google,
    Bing,
    Yahoo,
    StackOverflow,
    Custom,
}

impl SearchEngineSettings {
    /// The address that searches for `words` (encoded as a URL's query); `None` for a custom
    /// address that is not `http(s)://`.
    pub fn url(&self, words: &str) -> Option<String> {
        const WORD: &str = "$(CURRENT_WORD)";
        let template = match self.engine {
            SearchEngine::Duckduckgo => "https://duckduckgo.com/?q=$(CURRENT_WORD)",
            SearchEngine::Google => "https://www.google.com/search?q=$(CURRENT_WORD)",
            SearchEngine::Bing => "https://www.bing.com/search?q=$(CURRENT_WORD)",
            SearchEngine::Yahoo => "https://search.yahoo.com/search?p=$(CURRENT_WORD)",
            SearchEngine::StackOverflow => "https://stackoverflow.com/search?q=$(CURRENT_WORD)",
            SearchEngine::Custom => self.custom_url.trim(),
        };
        let lower = template.to_ascii_lowercase();
        if !(lower.starts_with("https://") || lower.starts_with("http://")) {
            return None;
        }
        let words = encode_query(words);
        Some(if template.contains(WORD) {
            template.replace(WORD, &words)
        } else {
            format!("{template}{words}")
        })
    }
}

/// `text` as a URL's query value: unreserved characters stay, spaces become `+`, the rest is
/// percent-encoded UTF-8.
pub(super) fn encode_query(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
