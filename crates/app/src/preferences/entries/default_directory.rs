//! Preferences > Default Directory.

use crate::preferences::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry(
        "Default Directory",
        "files.default-directory",
        "Open and Save As start in",
        Kind::Choice(&[
            ("follow", "The current document's folder"),
            ("remember", "The folder last used"),
            ("custom", "This folder"),
        ]),
    ),
    entry(
        "Default Directory",
        "files.default-directory-path",
        "Folder",
        Kind::Text,
    ),
    entry(
        "Default Directory",
        "files.drop-folder-opens-files",
        "A dropped folder opens its files, not Folder as Workspace",
        Kind::Bool,
    ),
];
