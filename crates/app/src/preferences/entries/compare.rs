//! Preferences > Compare.

use crate::preferences::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry(
        "Compare",
        "compare.align",
        "Align the lines both documents have",
        Kind::Bool,
    ),
    entry(
        "Compare",
        "compare.inline",
        "Mark what differs within a changed line",
        Kind::Bool,
    ),
    entry(
        "Compare",
        "compare.navigation-bar",
        "Show the differences along the scrollbar",
        Kind::Bool,
    ),
    entry(
        "Compare",
        "compare.recompare",
        "Compare again after changes",
        Kind::Bool,
    ),
];
