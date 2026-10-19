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
    pub toolbar: ToolbarSettings,
    pub instances: InstanceSettings,
    pub date: DateSettings,
    pub search_engine: SearchEngineSettings,
    pub links: LinkSettings,
    pub delimiters: DelimiterSettings,
    pub new_document: NewDocumentSettings,
    /// Tab size and spaces of a language, by its id (`python`), over `editor`'s
    /// ("Preferences > Language > Tab Settings").
    pub languages: std::collections::BTreeMap<String, LanguageSettings>,
    pub cloud: CloudSettings,
    pub session: SessionSettings,
    pub updates: UpdateSettings,
    pub plugins: PluginSettings,
    pub network: NetworkSettings,
    pub diagnostics: DiagnosticsSettings,
    pub preview: PreviewSettings,
}

/// How Birchpad looks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct AppearanceSettings {
    /// The color theme as Birchpad starts: a built-in one (`Default`, `Dark`) or the name of a
    /// file in the `themes` folder next to the settings, without its extension. Settings > Theme
    /// chooses another (remembered in `state.toml`).
    pub theme: String,
    /// The interface's language: `en`, `de`, `ru`, or `auto` for the system's.
    pub language: String,
    /// A Notepad++ translation (`localization/german.xml` of a Notepad++ folder) whose menu
    /// names and command titles come before the language's own; empty for none.
    pub native_lang: String,
    /// The title bar shows the file's name, not its whole path ("Show only filename in title
    /// bar").
    pub title_file_name_only: bool,
    /// The colors of menus, tabs, panels and dialogs, apart from the editor's theme: Notepad++'s
    /// Preferences > Dark Mode.
    pub interface: InterfaceMode,
}

/// Notepad++'s Dark Mode for the interface.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InterfaceMode {
    /// The theme's own interface colors.
    #[default]
    Theme,
    Light,
    Dark,
    /// Light or dark as the system is ("Follow Windows").
    System,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme: "Default".to_owned(),
            language: "auto".to_owned(),
            native_lang: String::new(),
            title_file_name_only: false,
            interface: InterfaceMode::default(),
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
    /// Show the words of the document while typing ("Word completion").
    pub words: bool,
    /// From how many characters of a word the document's words show.
    pub min_chars: u8,
    /// Show the language's API keywords and functions while typing ("Function completion").
    pub api: bool,
    /// From how many characters of a word the API's keywords show.
    pub api_min_chars: u8,
    /// Words and keywords match what is typed without regard to case.
    pub ignore_case: bool,
    /// Numbers are not offered as words.
    pub ignore_numbers: bool,
    /// Show a function's parameters after its opening bracket, from the language's API file.
    pub call_tips: bool,
    /// Typing the `>` of an HTML or XML opening tag inserts its closing tag after the caret.
    pub close_tags: bool,
}

impl Default for CompletionSettings {
    fn default() -> Self {
        Self {
            auto_close: "()[]{}\"\"''".to_owned(),
            words: true,
            min_chars: 1,
            api: true,
            api_min_chars: 1,
            ignore_case: true,
            ignore_numbers: true,
            call_tips: true,
            close_tags: false,
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
    /// A copy of the file as it was before each save ("Backup on save").
    pub backup_on_save: BackupOnSave,
    /// Where verbose backups go; empty: a `nppBackup` folder next to the file.
    pub backup_folder: String,
    /// Where Open and Save As start ("Default Directory").
    pub default_directory: DefaultDirectory,
    /// With `default-directory = "custom"`, the folder.
    pub default_directory_path: String,
    /// At startup, offer to make Birchpad the default editor of text files, unless it is.
    pub offer_default_app: bool,
}

/// Notepad++'s "Backup on save".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackupOnSave {
    #[default]
    None,
    /// `name.ext.bak` next to the file, replaced at every save.
    Simple,
    /// `name.ext.YYYY-MM-DD_HHMMSS.bak`, one for each save, in `backup-folder`.
    Verbose,
}

/// Where Open and Save As start.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DefaultDirectory {
    /// The active document's folder.
    #[default]
    Follow,
    /// The folder last used in a dialog.
    Remember,
    /// `default-directory-path`.
    Custom,
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
            backup_on_save: BackupOnSave::default(),
            backup_folder: String::new(),
            default_directory: DefaultDirectory::default(),
            default_directory_path: String::new(),
            offer_default_app: true,
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
    /// Closing the last tab quits ("Exit on close the last tab").
    pub exit_on_last_close: bool,
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
            exit_on_last_close: false,
        }
    }
}

/// The toolbar under the menus, as Notepad++'s (Preferences > General > Toolbar).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ToolbarSettings {
    /// Whether it shows.
    pub show: bool,
    pub size: ToolbarSize,
    /// Its buttons, in order: command ids, and `|` for a separator. Commands it does not know
    /// are left out.
    pub buttons: Vec<String>,
}

/// The toolbar's buttons as Notepad++ has them.
pub const DEFAULT_TOOLBAR: &[&str] = &[
    "file.new",
    "file.open",
    "file.save",
    "file.save-all",
    "file.close",
    "file.close-all",
    "file.print",
    "|",
    "edit.cut",
    "edit.copy",
    "edit.paste",
    "|",
    "edit.undo",
    "edit.redo",
    "|",
    "search.find",
    "search.replace",
    "|",
    "view.zoom-in",
    "view.zoom-out",
    "|",
    "view.sync-vertical-scroll",
    "view.sync-horizontal-scroll",
    "|",
    "view.word-wrap",
    "view.show-all-characters",
    "view.indent-guides",
    "|",
    "panel.document-map",
    "panel.document-list",
    "panel.function-list",
    "panel.folder-as-workspace",
    "|",
    "view.monitoring",
    "|",
    "macro.start-recording",
    "macro.stop-recording",
    "macro.playback",
    "macro.run-multiple",
    "macro.save",
];

impl Default for ToolbarSettings {
    fn default() -> Self {
        Self {
            show: true,
            size: ToolbarSize::default(),
            buttons: DEFAULT_TOOLBAR.iter().map(|&id| id.to_owned()).collect(),
        }
    }
}

/// How big the toolbar's icons are.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolbarSize {
    #[default]
    Small,
    Large,
}

/// Notepad++'s Preferences > Multi-Instance & Date.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct InstanceSettings {
    pub mode: InstanceMode,
}

/// What a launch does while Birchpad runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstanceMode {
    /// It hands its files to the running Birchpad ("Default (mono-instance)").
    #[default]
    Single,
    /// As `single`, but a session given with `-openSession` opens in a Birchpad of its own
    /// ("Open session in a new instance").
    Session,
    /// Every launch is a Birchpad of its own ("Always in multi-instance mode").
    Multi,
}

impl InstanceSettings {
    /// Whether a launch with `open_session` (`-openSession`) starts a Birchpad of its own.
    pub fn new_instance(&self, open_session: bool) -> bool {
        match self.mode {
            InstanceMode::Single => false,
            InstanceMode::Session => open_session,
            InstanceMode::Multi => true,
        }
    }
}

/// Edit > Insert > Date Time: Notepad++'s "Customize insert Date Time".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct DateSettings {
    /// Date Time (customized), in Notepad++'s letters: `yyyy-MM-dd HH:mm:ss`.
    pub custom_format: String,
    /// The date before the time in the short and long formats ("Reverse default date time
    /// order").
    pub reverse: bool,
}

impl Default for DateSettings {
    fn default() -> Self {
        Self {
            custom_format: "yyyy-MM-dd HH:mm:ss".to_owned(),
            reverse: false,
        }
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
fn encode_query(text: &str) -> String {
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

/// Clickable links: Notepad++'s Preferences > Cloud & Link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct LinkSettings {
    /// Addresses can be opened with a double click or Ctrl+click.
    pub enabled: bool,
    /// They are underlined.
    pub underline: bool,
    /// Schemes besides http, https, ftp, ftps, file and mailto, separated by spaces
    /// ("URI customized schemes").
    pub schemes: String,
}

impl Default for LinkSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            underline: true,
            schemes: "svn:// git:// ssh:// sftp://".to_owned(),
        }
    }
}

impl LinkSettings {
    /// The schemes of one's own.
    pub fn extra_schemes(&self) -> Vec<String> {
        self.schemes.split_whitespace().map(str::to_owned).collect()
    }
}

/// Ctrl+double click selects between these: Notepad++'s Preferences > Delimiter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct DelimiterSettings {
    /// One character each; anything else turns the selection off.
    pub open: String,
    pub close: String,
    /// The delimiters may be on other lines ("Allow on several lines").
    pub multi_line: bool,
}

impl Default for DelimiterSettings {
    fn default() -> Self {
        Self {
            open: "\"".to_owned(),
            close: "\"".to_owned(),
            multi_line: false,
        }
    }
}

impl DelimiterSettings {
    /// The opening and the closing character, if both are one character.
    pub fn pair(&self) -> Option<(char, char)> {
        let one = |text: &str| {
            let mut chars = text.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Some(c),
                _ => None,
            }
        };
        Some((one(&self.open)?, one(&self.close)?))
    }
}

/// Notepad++'s Preferences > New Document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct NewDocumentSettings {
    /// As the Encoding menu names them: `utf-8`, `utf-8-bom`, `utf-16le-bom`, `ansi`...
    pub encoding: String,
    /// `crlf`, `lf` or `cr`; `system`: the platform's.
    pub line_ending: String,
    /// A language id, or `text`.
    pub language: String,
}

impl Default for NewDocumentSettings {
    fn default() -> Self {
        Self {
            encoding: "utf-8".to_owned(),
            line_ending: "system".to_owned(),
            language: "text".to_owned(),
        }
    }
}

/// One language's tab settings; unset ones are `editor`'s.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct LanguageSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab_width: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert_spaces: Option<bool>,
}

/// Settings kept in a folder of one's choice (Notepad++'s Preferences > Cloud & Link).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct CloudSettings {
    /// The folder whose `settings.toml` (and `keymap.toml`, themes...) Birchpad uses from the
    /// next start; empty: the usual place. Only the local settings file can set it.
    pub folder: String,
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
    /// Files of the session that cannot be read now (a network drive away) stay in it, to open
    /// when they can ("Remember inaccessible files").
    pub remember_inaccessible: bool,
}

impl Default for SessionSettings {
    fn default() -> Self {
        Self {
            remember: true,
            backup_unsaved: true,
            backup_interval_seconds: 7,
            remember_inaccessible: false,
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

/// The previews beside the text (View > Preview).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct PreviewSettings {
    /// What draws PlantUML diagrams: the `plantuml` command (found on `PATH` by its name, or
    /// a path to it), or a path to `plantuml.jar`, run with `java -jar`.
    pub plantuml: String,
}

impl Default for PreviewSettings {
    fn default() -> Self {
        Self {
            plantuml: "plantuml".to_owned(),
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
mod link_tests {
    use super::*;

    #[test]
    fn schemes_and_delimiters() {
        let links = LinkSettings::default();
        assert_eq!(
            links.extra_schemes(),
            ["svn://", "git://", "ssh://", "sftp://"]
        );
        let none = LinkSettings {
            schemes: "  ".into(),
            ..LinkSettings::default()
        };
        assert!(none.extra_schemes().is_empty());
        assert_eq!(DelimiterSettings::default().pair(), Some(('"', '"')));
        let brackets = DelimiterSettings {
            open: "(".into(),
            close: ")".into(),
            multi_line: true,
        };
        assert_eq!(brackets.pair(), Some(('(', ')')));
        for (open, close) in [("", ")"), ("((", ")"), ("(", "")] {
            let wrong = DelimiterSettings {
                open: open.into(),
                close: close.into(),
                multi_line: false,
            };
            assert_eq!(wrong.pair(), None, "{open:?} {close:?}");
        }
    }
}

#[cfg(test)]
mod search_engine_tests {
    use super::*;

    #[test]
    fn engines_search_for_the_words() {
        let mut settings = SearchEngineSettings::default();
        assert_eq!(
            settings.url("rust fmt").as_deref(),
            Some("https://duckduckgo.com/?q=rust+fmt")
        );
        settings.engine = SearchEngine::Google;
        assert_eq!(
            settings.url("a&b=c/d").as_deref(),
            Some("https://www.google.com/search?q=a%26b%3Dc%2Fd")
        );
        settings.engine = SearchEngine::StackOverflow;
        assert_eq!(
            settings.url("é").as_deref(),
            Some("https://stackoverflow.com/search?q=%C3%A9")
        );
        assert_eq!(
            settings.url("").as_deref(),
            Some("https://stackoverflow.com/search?q=")
        );
    }

    #[test]
    fn a_custom_address() {
        let mut settings = SearchEngineSettings {
            engine: SearchEngine::Custom,
            custom_url: " https://example.com/s?w=$(CURRENT_WORD)&x=1 ".into(),
        };
        assert_eq!(
            settings.url("a b").as_deref(),
            Some("https://example.com/s?w=a+b&x=1")
        );
        // Without the placeholder, the words go at the end.
        settings.custom_url = "HTTP://example.com/?q=".into();
        assert_eq!(
            settings.url("x").as_deref(),
            Some("HTTP://example.com/?q=x")
        );
        // Only web addresses.
        for bad in [
            "",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "example.com",
        ] {
            settings.custom_url = bad.into();
            assert_eq!(settings.url("x"), None, "{bad}");
        }
    }
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
