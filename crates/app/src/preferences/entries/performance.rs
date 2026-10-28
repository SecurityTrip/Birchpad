//! Preferences > Performance.

use crate::preferences::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry(
        "Performance",
        "files.large-file-no-wrap",
        "Large files: word wrap off everywhere",
        Kind::Bool,
    ),
    entry(
        "Performance",
        "files.large-file-completion",
        "Large files keep auto-completion",
        Kind::Bool,
    ),
    entry(
        "Performance",
        "files.large-file-smart-highlighting",
        "Large files keep smart highlighting",
        Kind::Bool,
    ),
    entry(
        "Performance",
        "files.large-file-brace-match",
        "Large files keep brace matching",
        Kind::Bool,
    ),
    entry(
        "Performance",
        "files.large-file-links",
        "Large files keep clickable links",
        Kind::Bool,
    ),
];
