//! Application state that is not a setting: recently opened files, the zoom level. It lives in
//! `state.toml` in the user data directory, separate from `settings.toml`, so that the settings
//! file only changes when the user changes a setting.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The most recent files any setting can ask to remember.
pub const MAX_RECENT_FILES: usize = 100;

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
    /// How split view places the two views, as last rotated.
    pub split: SplitOrientation,
    /// What update checks remember.
    pub updates: UpdateState,
    /// The fields and options of Find in Files as last used.
    pub find_in_files: FindInFilesState,
    /// The side panels.
    pub panels: PanelsState,
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
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_files_are_most_recent_first_and_limited() {
        let mut state = UserState::default();
        for name in ["a", "b", "c", "b"] {
            state.add_recent(Path::new(name), 3);
        }
        assert_eq!(
            state.recent_files,
            [Path::new("b"), Path::new("c"), Path::new("a")]
        );
        state.add_recent(Path::new("d"), 2);
        assert_eq!(state.recent_files, [Path::new("d"), Path::new("b")]);
        state.remove_recent(Path::new("d"));
        assert_eq!(state.recent_files, [Path::new("b")]);
        state.add_recent(Path::new("e"), 0);
        assert!(state.recent_files.is_empty());
    }

    #[test]
    fn round_trips_and_survives_damage() {
        let dir = std::env::temp_dir().join(format!("birchpad-state-{}", std::process::id()));
        let path = dir.join("state.toml");
        let mut state = UserState {
            zoom: 2,
            updates: UpdateState {
                last_check: 1_790_000_000,
                manifest_timestamp: 1_789_000_000,
                announced: Some("0.2.0".into()),
            },
            ..UserState::default()
        };
        state.add_recent(Path::new("/tmp/x y.txt"), 10);
        state.save(&path).unwrap();
        assert_eq!(UserState::load(&path), state);

        fs::write(&path, "recent-files = 7").unwrap();
        assert_eq!(UserState::load(&path), UserState::default());
        assert_eq!(
            UserState::load(&dir.join("missing.toml")),
            UserState::default()
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn find_in_files_options_round_trip_and_default_when_missing() {
        let defaults: UserState = toml::from_str("zoom = 1").unwrap();
        assert_eq!(defaults.find_in_files, FindInFilesState::default());
        assert!(defaults.find_in_files.subfolders, "Notepad++'s default");
        // Some keys given: the others keep their defaults.
        let partial: UserState = toml::from_str("[find-in-files]\nhidden = true").unwrap();
        assert!(partial.find_in_files.hidden && partial.find_in_files.subfolders);
        let state = UserState {
            find_in_files: FindInFilesState {
                filters: r"*.rs !+\target".into(),
                directory: Some(PathBuf::from("/projects/birchpad")),
                subfolders: false,
                hidden: true,
                follow_current_document: true,
            },
            ..UserState::default()
        };
        let text = toml::to_string(&state).unwrap();
        assert_eq!(toml::from_str::<UserState>(&text).unwrap(), state);
        // A wrong type is damage: the whole state is the default, as for any other key.
        assert!(toml::from_str::<UserState>("[find-in-files]\nsubfolders = 3").is_err());
    }
}
