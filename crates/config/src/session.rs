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
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("cannot read {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("{0}")]
    Format(String),
    #[error("the session was saved by a newer version of Birchpad (format {0})")]
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
mod tests {
    use super::*;

    fn sample() -> Session {
        let mut session = Session::new();
        session.active_view = 1;
        session.documents = vec![
            SessionDocument {
                path: Some(PathBuf::from("/home/me/notes.txt")),
                backup: Some("notes.txt@1".into()),
                modified: true,
                encoding: Some("windows-1251".into()),
                line_ending: Some("crlf".into()),
                bookmarks: vec![2, 7],
                ..SessionDocument::default()
            },
            SessionDocument {
                untitled: Some(3),
                backup: Some("new 3@2".into()),
                modified: true,
                language: Some("rust".into()),
                ..SessionDocument::default()
            },
        ];
        session.main_view.tabs = vec![SessionTab {
            document: 0,
            selections: vec![[3, 9], [12, 12]],
            primary: 1,
            first_row: 4.5,
            scroll_x: 16.,
            folds: vec![1],
        }];
        session.second_view = SessionView {
            active: 1,
            tabs: vec![
                SessionTab {
                    document: 1,
                    ..SessionTab::default()
                },
                SessionTab {
                    document: 0,
                    ..SessionTab::default()
                },
            ],
        };
        session
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("birchpad-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn round_trips() {
        let session = sample();
        assert_eq!(Session::parse(&session.to_toml()).unwrap(), session);
        assert_eq!(
            session.backup_names().collect::<Vec<_>>(),
            ["notes.txt@1", "new 3@2"]
        );
    }

    #[test]
    fn either_format_reads_with_or_without_a_byte_order_mark() {
        let toml = sample().to_toml();
        let xml = r#"<NotepadPlus><Session activeView="0"><mainView activeIndex="0">
            <File filename="C:\a.txt" /></mainView></Session></NotepadPlus>"#;
        for text in [toml.clone(), format!("\u{feff}{toml}")] {
            assert_eq!(Session::from_text(&text).unwrap(), sample());
        }
        for text in [xml.to_owned(), format!("\u{feff}  {xml}")] {
            let session = Session::from_text(&text).unwrap();
            assert_eq!(session.documents.len(), 1);
        }
        // One mark only: a second is text, which neither format starts with.
        assert!(Session::from_text(&format!("\u{feff}\u{feff}{xml}")).is_err());
        assert!(Session::from_text("").is_err());
        assert!(Session::from_text("\u{feff}").is_err());
    }

    #[test]
    fn rejects_other_files_and_newer_formats() {
        assert!(Session::parse("recent-files = []").is_err());
        assert!(matches!(
            Session::parse("version = 99"),
            Err(SessionError::TooNew(99))
        ));
    }

    #[test]
    fn a_damaged_session_falls_back_to_the_previous_one() {
        let dir = temp_dir("session");
        let path = dir.join("session.toml");
        let mut first = sample();
        first.active_view = 0;
        first.save(&path).unwrap();
        let second = sample();
        second.save(&path).unwrap();
        assert_eq!(Session::load(&path), Some(second));
        assert_eq!(Session::read(&previous(&path)).unwrap(), first);

        // Cut short (a crash in the middle of copying it, a full disk): the previous one loads.
        let text = fs::read_to_string(&path).unwrap();
        let cut = text.find("notes.txt@1").unwrap() + 4;
        fs::write(&path, &text[..cut]).unwrap();
        assert_eq!(Session::load(&path), Some(first.clone()));

        // Missing, as after a crash between the two renames of a save.
        fs::remove_file(&path).unwrap();
        assert_eq!(Session::load(&path), Some(first));
        // A temporary file left by an interrupted write is never read.
        fs::write(
            path.with_extension("toml.tmp"),
            "version = 1\nactive-view = 1",
        )
        .unwrap();
        fs::remove_file(previous(&path)).unwrap();
        assert_eq!(Session::load(&path), None);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn private_files_replace_atomically() {
        let dir = temp_dir("private");
        let path = dir.join("backup").join("new 1@5");
        write_private(&path, b"one").unwrap();
        write_private(&path, b"two").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"two");
        assert!(!dir.join("backup").join("new 1@5.tmp").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&path), 0o600);
            assert_eq!(mode(&dir.join("backup")), 0o700);
        }
        fs::remove_dir_all(&dir).unwrap();
    }
}
