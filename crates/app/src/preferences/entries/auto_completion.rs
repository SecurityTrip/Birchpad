//! Preferences > Auto-Completion.

use crate::preferences::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry(
        "Auto-Completion",
        "completion.api",
        "Function completion",
        Kind::Bool,
    ),
    entry(
        "Auto-Completion",
        "completion.api-min-chars",
        "Functions from character",
        Kind::Number { min: 1, max: 9 },
    ),
    entry(
        "Auto-Completion",
        "completion.words",
        "Word completion",
        Kind::Bool,
    ),
    entry(
        "Auto-Completion",
        "completion.min-chars",
        "From character",
        Kind::Number { min: 1, max: 9 },
    ),
    entry(
        "Auto-Completion",
        "completion.ignore-case",
        "Ignore case",
        Kind::Bool,
    ),
    entry(
        "Auto-Completion",
        "completion.ignore-numbers",
        "Ignore numbers",
        Kind::Bool,
    ),
    entry(
        "Auto-Completion",
        "completion.call-tips",
        "Function parameters hint",
        Kind::Bool,
    ),
    entry(
        "Auto-Completion",
        "completion.auto-close",
        "Auto-close pairs",
        Kind::Text,
    ),
    entry(
        "Auto-Completion",
        "completion.close-tags",
        "Close HTML and XML tags",
        Kind::Bool,
    ),
    entry(
        "Auto-Completion",
        "completion.accept-with-tab",
        "Insert the selection with Tab",
        Kind::Bool,
    ),
    entry(
        "Auto-Completion",
        "completion.accept-with-enter",
        "Insert the selection with Enter",
        Kind::Bool,
    ),
    entry(
        "Auto-Completion",
        "completion.brief",
        "Make the auto-completion list brief",
        Kind::Bool,
    ),
    entry(
        "Auto-Completion",
        "editor.backspace-unindents",
        "Backspace unindents",
        Kind::Bool,
    ),
];
