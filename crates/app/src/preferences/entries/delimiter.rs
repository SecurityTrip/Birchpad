//! Preferences > Delimiter.

use crate::preferences::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry("Delimiter", "delimiters.open", "Open", Kind::Text),
    entry("Delimiter", "delimiters.close", "Close", Kind::Text),
    entry(
        "Delimiter",
        "delimiters.multi-line",
        "Allow on several lines",
        Kind::Bool,
    ),
    entry(
        "Delimiter",
        "delimiters.word-chars",
        "Characters that are part of words",
        Kind::Text,
    ),
];
