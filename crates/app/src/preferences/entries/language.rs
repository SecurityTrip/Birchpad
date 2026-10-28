//! Preferences > Language.

use crate::preferences::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry("Language", "languages", "Tab settings", Kind::LanguageTabs),
    entry(
        "Language",
        "language-menu.compact",
        "Make the language menu compact",
        Kind::Bool,
    ),
    entry(
        "Language",
        "language-menu.hidden",
        "Languages hidden from the menu (ids)",
        Kind::Text,
    ),
    entry(
        "Language",
        "sql.backslash-escapes",
        "Treat backslash as escape character for SQL",
        Kind::Bool,
    ),
];
