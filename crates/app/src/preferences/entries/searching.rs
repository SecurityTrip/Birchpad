//! Preferences > Searching.

use crate::preferences::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry(
        "Searching",
        "searching.in-selection-from",
        "In selection from (characters, 0: never)",
        Kind::Number {
            min: 0,
            max: 1000000,
        },
    ),
    entry(
        "Searching",
        "searching.fill-from-selection",
        "Fill the find field from a selection of up to (characters)",
        Kind::Number { min: 0, max: 16383 },
    ),
    entry(
        "Searching",
        "searching.fill-from-word",
        "Fill the find field with the word at the caret",
        Kind::Bool,
    ),
    entry(
        "Searching",
        "searching.monospaced",
        "Monospaced font in the find panel",
        Kind::Bool,
    ),
    entry(
        "Searching",
        "searching.stay-open",
        "The find panel stays open after Find All",
        Kind::Bool,
    ),
    entry(
        "Searching",
        "searching.confirm-replace-all-documents",
        "Confirm Replace All in All Opened Documents",
        Kind::Bool,
    ),
    entry(
        "Searching",
        "searching.replace-stays",
        "Replace: don't move to the next match",
        Kind::Bool,
    ),
    entry(
        "Searching",
        "searching.one-entry-per-line",
        "Search results: one entry per line",
        Kind::Bool,
    ),
    entry(
        "Searching",
        "searching.ignore-unsaved-changes",
        "Find in Files: ignore unsaved changes",
        Kind::Bool,
    ),
    entry(
        "Searching",
        "searching.folder-from-document",
        "Find in Files: the document's folder",
        Kind::Bool,
    ),
];
