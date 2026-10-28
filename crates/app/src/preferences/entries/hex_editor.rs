//! Preferences > Hex Editor.

use crate::preferences::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry(
        "Hex Editor",
        "hex.columns",
        "Bytes in a row",
        Kind::Number { min: 1, max: 64 },
    ),
    entry(
        "Hex Editor",
        "hex.group",
        "Bytes in a group",
        Kind::Number { min: 1, max: 64 },
    ),
    entry(
        "Hex Editor",
        "hex.uppercase",
        "Hexadecimal digits in capitals",
        Kind::Bool,
    ),
];
