//! Preferences > Highlighting.

use crate::preferences::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry(
        "Highlighting",
        "highlighting.smart.enabled",
        "Smart highlighting",
        Kind::Bool,
    ),
    entry(
        "Highlighting",
        "highlighting.smart.match-case",
        "Match case",
        Kind::Bool,
    ),
    entry(
        "Highlighting",
        "highlighting.smart.whole-word",
        "Whole word only",
        Kind::Bool,
    ),
    entry(
        "Highlighting",
        "highlighting.smart.use-find-options",
        "Use the find panel's options",
        Kind::Bool,
    ),
    entry(
        "Highlighting",
        "highlighting.token-style.match-case",
        "Style tokens: match case",
        Kind::Bool,
    ),
    entry(
        "Highlighting",
        "highlighting.token-style.whole-word",
        "Style tokens: whole word only",
        Kind::Bool,
    ),
    entry(
        "Highlighting",
        "highlighting.smart-other-view",
        "Smart highlighting in the other view too",
        Kind::Bool,
    ),
    entry(
        "Highlighting",
        "highlighting.matching-tags",
        "Highlight matching tags",
        Kind::Bool,
    ),
    entry(
        "Highlighting",
        "highlighting.matching-tag-attributes",
        "Highlight the tags with their attributes",
        Kind::Bool,
    ),
    entry(
        "Highlighting",
        "highlighting.matching-tags-in-comments",
        "Highlight matching tags in comments",
        Kind::Bool,
    ),
];
