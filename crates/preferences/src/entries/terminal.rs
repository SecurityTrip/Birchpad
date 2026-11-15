//! Preferences > Terminal (ADR 0067).

use crate::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry(
        "Terminal",
        "terminal.shell",
        "Shell (empty: the system's)",
        Kind::Text,
    ),
    entry(
        "Terminal",
        "terminal.scrollback",
        "Scrollback lines",
        Kind::Number {
            min: 0,
            max: birchpad_config::MAX_SCROLLBACK as i64,
        },
    ),
];
