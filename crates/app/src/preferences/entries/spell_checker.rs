//! Preferences > Spell Checker.

use crate::preferences::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry(
        "Spell Checker",
        "spell.languages",
        "Languages checked together",
        Kind::Text,
    ),
    entry(
        "Spell Checker",
        "spell.check",
        "In code, check",
        Kind::Choice(&[
            ("comments-and-strings", "Comments and strings"),
            ("comments", "Comments"),
            ("strings", "Strings"),
            ("everything", "Everything"),
        ]),
    ),
    entry(
        "Spell Checker",
        "spell.ignore-numbers",
        "Ignore words with digits",
        Kind::Bool,
    ),
    entry(
        "Spell Checker",
        "spell.ignore-all-caps",
        "Ignore words in capitals",
        Kind::Bool,
    ),
    entry(
        "Spell Checker",
        "spell.ignore-capitals-inside",
        "Ignore words with a capital inside",
        Kind::Bool,
    ),
    entry(
        "Spell Checker",
        "spell.ignore-capital-start",
        "Ignore words that start with a capital",
        Kind::Bool,
    ),
    entry(
        "Spell Checker",
        "spell.ignore-underscores",
        "Ignore words with underscores",
        Kind::Bool,
    ),
    entry(
        "Spell Checker",
        "spell.ignore-one-letter",
        "Ignore single letters",
        Kind::Bool,
    ),
    entry(
        "Spell Checker",
        "spell.ignore-addresses",
        "Ignore addresses",
        Kind::Bool,
    ),
];
