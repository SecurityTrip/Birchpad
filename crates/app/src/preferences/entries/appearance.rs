//! Preferences > Appearance.

use crate::preferences::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry(
        "Appearance",
        "appearance.language",
        "Interface language",
        Kind::Choice(&[
            ("auto", "Automatic (system)"),
            ("en", "English"),
            ("de", "Deutsch"),
            ("ru", "Русский"),
        ]),
    ),
    entry(
        "Appearance",
        "appearance.native-lang",
        "Notepad++ translation file",
        Kind::Text,
    ),
    entry(
        "Appearance",
        "appearance.interface",
        "Interface (Dark Mode)",
        Kind::Choice(&[
            ("theme", "As the theme"),
            ("light", "Light"),
            ("dark", "Dark"),
            ("system", "As the system"),
        ]),
    ),
    entry(
        "Appearance",
        "appearance.title-file-name-only",
        "Show only the file name in the title bar",
        Kind::Bool,
    ),
    entry(
        "Appearance",
        "appearance.dark-tone",
        "Dark Mode tone",
        Kind::Choice(&[
            ("black", "Black"),
            ("red", "Red"),
            ("green", "Green"),
            ("blue", "Blue"),
            ("purple", "Purple"),
            ("cyan", "Cyan"),
            ("olive", "Olive"),
            ("custom", "Customized"),
        ]),
    ),
    entry(
        "Appearance",
        "appearance.dark-tone-colors.background",
        "Customized tone: background",
        Kind::Text,
    ),
    entry(
        "Appearance",
        "appearance.dark-tone-colors.surface",
        "Customized tone: surface",
        Kind::Text,
    ),
    entry(
        "Appearance",
        "appearance.dark-tone-colors.border",
        "Customized tone: border",
        Kind::Text,
    ),
    entry(
        "Appearance",
        "appearance.dark-tone-colors.text",
        "Customized tone: text",
        Kind::Text,
    ),
    entry(
        "Appearance",
        "appearance.dark-tone-colors.muted",
        "Customized tone: muted",
        Kind::Text,
    ),
    entry(
        "Appearance",
        "appearance.dark-tone-colors.faint",
        "Customized tone: faint",
        Kind::Text,
    ),
    entry(
        "Appearance",
        "appearance.dark-tone-colors.selected",
        "Customized tone: selected",
        Kind::Text,
    ),
    entry(
        "Appearance",
        "appearance.dark-tone-colors.hovered",
        "Customized tone: hovered",
        Kind::Text,
    ),
    entry(
        "Appearance",
        "appearance.dark-tone-colors.accent",
        "Customized tone: accent",
        Kind::Text,
    ),
    entry(
        "Appearance",
        "appearance.dark-tone-colors.error",
        "Customized tone: error",
        Kind::Text,
    ),
    entry(
        "Appearance",
        "appearance.menu-bar",
        "Show the menu bar (hidden, Alt or F10 shows it)",
        Kind::Bool,
    ),
    entry(
        "Appearance",
        "appearance.status-bar",
        "Show the status bar",
        Kind::Bool,
    ),
];
