//! Preferences > Toolbar.

use crate::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry("Toolbar", "toolbar.show", "Show the toolbar", Kind::Bool),
    entry(
        "Toolbar",
        "toolbar.size",
        "Icons",
        Kind::Choice(&[("small", "Small"), ("large", "Large")]),
    ),
];
