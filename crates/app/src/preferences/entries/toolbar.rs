//! Preferences > Toolbar.

use crate::preferences::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry("Toolbar", "toolbar.show", "Show the toolbar", Kind::Bool),
    entry(
        "Toolbar",
        "toolbar.size",
        "Icons",
        Kind::Choice(&[("small", "Small"), ("large", "Large")]),
    ),
];
