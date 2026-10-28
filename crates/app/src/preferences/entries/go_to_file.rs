//! Preferences > Go to File.

use crate::preferences::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry(
        "Go to File",
        "finder.filters",
        "Files listed (Find in Files' filters)",
        Kind::Text,
    ),
    entry(
        "Go to File",
        "finder.hidden",
        "Hidden files too",
        Kind::Bool,
    ),
    entry(
        "Go to File",
        "finder.max-files",
        "Files listed at most",
        Kind::Number {
            min: 1000,
            max: 1_000_000,
        },
    ),
];
