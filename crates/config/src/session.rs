//! Sessions: the documents open in the window, where each view of them was, and which unsaved
//! changes have a backup copy, so that the next launch puts everything back (Notepad++'s
//! session snapshot).
//!
//! A session is a versioned TOML file. It is written atomically (a temporary file renamed over
//! the old one) and readable only by the user. The previous session is kept next to it: a
//! session file that is damaged or missing loads the previous one instead.

use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The session format this version writes and the newest it reads.
pub const SESSION_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Session {
    pub version: u32,
    /// The view that had the focus: 0 for the main view, 1 for the second.
    pub active_view: usize,
    pub documents: Vec<SessionDocument>,
    pub main_view: SessionView,
    pub second_view: SessionView,
}

/// An open document.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct SessionDocument {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// The number of an untitled document: 1 for "new 1".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub untitled: Option<usize>,
    /// The name an untitled document was given instead of "new N".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The text, as UTF-8, when it is not the file's: a file name in the backup folder, or an
    /// absolute path (sessions imported from Notepad++).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup: Option<String>,
    /// The text differs from the file.
    pub modified: bool,
    /// With a backup, the encoding to save in (`utf-8-bom`, `windows-1251`...). Without one,
    /// the encoding the file was reopened in from the Encoding menu, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding: Option<String>,
    /// With a backup, the line ending of new lines: `crlf`, `lf` or `cr`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_ending: Option<String>,
    /// The language chosen in the Language menu, if any (an id, or a Notepad++ language name
    /// in imported sessions).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub read_only: bool,
    /// Bookmarked lines.
    pub bookmarks: Vec<usize>,
    /// With a backup of a file, the file's size and modification time (nanoseconds since
    /// 1970) then: if it changed meanwhile, the user is asked about it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_len: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_modified: Option<u64>,
}

/// The tabs of a view.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct SessionView {
    /// The index of the active tab.
    pub active: usize,
    pub tabs: Vec<SessionTab>,
}

/// A tab: a view of one of the documents.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct SessionTab {
    /// Index into [`Session::documents`].
    pub document: usize,
    /// Selections as `[anchor, head]` byte offsets.
    pub selections: Vec<[usize; 2]>,
    /// Index of the primary selection.
    pub primary: usize,
    /// The first visible row, fractional while scrolled smoothly.
    pub first_row: f64,
    /// Horizontal scroll offset in pixels.
    pub scroll_x: f32,
    /// First lines of the collapsed folds.
    pub folds: Vec<usize>,
    /// The tab is pinned: it stays at the start of the tab bar.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub pinned: bool,
    /// The tab's color, 1 to 5 as Notepad++'s Apply Color 1-5; none if unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<u8>,
    /// Edit > Text Direction RTL: the text runs right to left (Notepad++'s `RTL`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub rtl: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("{}", birchpad_i18n::t!("config.cannot-read", path = .path.display(), reason = .source))]
    Read { path: PathBuf, source: io::Error },
    #[error("{0}")]
    Format(String),
    #[error("{}", birchpad_i18n::t!("config.session-too-new", format = .0))]
    TooNew(u32),
}

impl Session {
    /// An empty session in the current format.
    pub fn new() -> Self {
        Self {
            version: SESSION_VERSION,
            ..Self::default()
        }
    }

    pub fn views(&self) -> [&SessionView; 2] {
        [&self.main_view, &self.second_view]
    }

    pub fn views_mut(&mut self) -> [&mut SessionView; 2] {
        [&mut self.main_view, &mut self.second_view]
    }

    pub fn to_toml(&self) -> String {
        toml::to_string(self).expect("sessions serialize")
    }

    /// Parses a session file in Birchpad's format.
    pub fn parse(text: &str) -> Result<Self, SessionError> {
        let session: Self =
            toml::from_str(text).map_err(|error| SessionError::Format(error.to_string()))?;
        match session.version {
            0 => Err(SessionError::Format(
                "not a session: no format version".into(),
            )),
            version if version > SESSION_VERSION => Err(SessionError::TooNew(version)),
            _ => Ok(session),
        }
    }

    /// Reads a session file: Birchpad's own, or a Notepad++ `session.xml`.
    pub fn read(path: &Path) -> Result<Self, SessionError> {
        let text = fs::read_to_string(path).map_err(|source| SessionError::Read {
            path: path.to_owned(),
            source,
        })?;
        Self::from_text(&text)
    }

    /// Parses the text of a session file in either format, after a byte order mark if an
    /// editor added one.
    pub fn from_text(text: &str) -> Result<Self, SessionError> {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        if text.trim_start().starts_with('<') {
            crate::notepad_session::import(text)
        } else {
            Self::parse(text)
        }
    }

    /// Loads the session saved at `path`, or the previous one if that file is missing or
    /// damaged. `None` if neither loads.
    pub fn load(path: &Path) -> Option<Self> {
        [path.to_owned(), previous(path)]
            .iter()
            .find_map(|path| Self::read(path).ok())
    }

    /// Saves the session at `path`, keeping the session it replaces as the previous one.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let temporary = path.with_extension("toml.tmp");
        write_new_private(&temporary, self.to_toml().as_bytes())?;
        if path.exists() {
            fs::rename(path, previous(path))?;
        }
        fs::rename(&temporary, path)
    }

    /// Names in the backup folder this session uses.
    pub fn backup_names(&self) -> impl Iterator<Item = &str> {
        self.documents
            .iter()
            .filter_map(|document| document.backup.as_deref())
            .filter(|backup| !Path::new(backup).is_absolute())
    }
}

/// Where the session replaced by the last save is kept.
fn previous(path: &Path) -> PathBuf {
    path.with_extension("toml.bak")
}

/// Replaces `path` with `bytes` atomically, readable only by the user. The folder is created
/// if needed, also only for the user.
pub fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("no file name"))?
        .to_string_lossy();
    let temporary = path.with_file_name(format!("{name}.tmp"));
    write_new_private(&temporary, bytes)?;
    fs::rename(&temporary, path)
}

/// Writes a file that only the user can read, flushed to the disk.
fn write_new_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        create_private_dir(dir)?;
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Creates a folder (and its parents) that only the user can enter. On Windows the folder
/// inherits the permissions of the user's profile.
pub fn create_private_dir(dir: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        builder.mode(0o700);
    }
    builder.create(dir)
}

#[cfg(test)]
#[path = "../tests/unit/session/tests.rs"]
mod tests;
