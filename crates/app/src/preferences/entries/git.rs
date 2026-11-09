//! Preferences > Git (ADR 0066).

use crate::preferences::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry("Git", "git.enabled", "Use Git", Kind::Bool),
    entry("Git", "git.program", "Git program", Kind::Text),
];
