//! Preferences > Command Palette.

use crate::preferences::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[entry(
    "Command Palette",
    "palette.recent-commands",
    "Recently used commands listed first",
    Kind::Number { min: 0, max: 50 },
)];
