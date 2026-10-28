//! Files and documents: Preferences > Files, Backup, New Document, and the session.

use serde::{Deserialize, Serialize};

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
    /// File > Recent Files lists the files in a submenu of their own ("In Submenu").
    pub recent_submenu: bool,
    /// The recent files show by their name only ("Only File Name").
    pub recent_file_name_only: bool,
    /// Recent paths longer than this many characters are cut in the middle; 0 for no limit
    /// ("Customize Maximum Length").
    pub recent_max_length: u16,
    /// Recent files that are gone are left out of the menu ("Don't check at launch time" off).
    pub recent_check_existence: bool,
    /// A folder dropped on the window opens its files instead of going to Folder as Workspace.
    pub drop_folder_opens_files: bool,
    /// Large files: word wrap goes off for every document when one opens ("Deactivate Word
    /// Wrap globally").
    pub large_file_no_wrap: bool,
    /// What large files keep: auto-completion, smart highlighting, brace matching, links.
    pub large_file_completion: bool,
    pub large_file_smart_highlighting: bool,
    pub large_file_brace_match: bool,
    pub large_file_links: bool,
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
            recent_submenu: false,
            recent_file_name_only: false,
            recent_max_length: 0,
            recent_check_existence: true,
            drop_folder_opens_files: false,
            large_file_no_wrap: false,
            large_file_completion: false,
            large_file_smart_highlighting: false,
            large_file_brace_match: true,
            large_file_links: true,
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
    /// Files opened in the ANSI encoding with nothing but ASCII in them take the new
    /// document's encoding ("Apply to opened ANSI files").
    pub apply_to_ansi: bool,
    /// A new document opens at startup besides the session and the files asked for.
    pub at_startup: bool,
    /// An untitled document's tab is named after its first line.
    pub first_line_name: bool,
}

impl Default for NewDocumentSettings {
    fn default() -> Self {
        Self {
            encoding: "utf-8".to_owned(),
            line_ending: "system".to_owned(),
            language: "text".to_owned(),
            apply_to_ansi: false,
            at_startup: false,
            first_line_name: false,
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
    /// Files of the session that cannot be read now (a network drive away) stay in it, to open
    /// when they can ("Remember inaccessible files").
    pub remember_inaccessible: bool,
    /// Files with this extension (`.npss`) open as sessions, not as text.
    pub session_extension: String,
    /// Files with this extension open their folders as workspace, not as text.
    pub workspace_extension: String,
}

impl Default for SessionSettings {
    fn default() -> Self {
        Self {
            remember: true,
            backup_unsaved: true,
            backup_interval_seconds: 7,
            remember_inaccessible: false,
            session_extension: String::new(),
            workspace_extension: String::new(),
        }
    }
}
