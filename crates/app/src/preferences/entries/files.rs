//! Preferences > Files.

use crate::preferences::{EXPLORER_MENU, Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry(
        "Files",
        "files.recent-limit",
        "Recent files",
        Kind::Number { min: 0, max: 100 },
    ),
    entry("Files", "files.ansi-encoding", "ANSI encoding", Kind::Text),
    entry(
        "Files",
        "files.large-file-limit-mb",
        "Large file limit (MB)",
        Kind::Number {
            min: 1,
            max: 1_048_576,
        },
    ),
    entry(
        "Files",
        "files.change-detection",
        "Detect changes by other programs",
        Kind::Choice(&[
            ("all", "All files"),
            ("current", "Current file"),
            ("off", "Off"),
        ]),
    ),
    entry(
        "Files",
        "files.reload-silently",
        "Reload without asking",
        Kind::Bool,
    ),
    entry(
        "Files",
        "files.reload-scrolls-to-end",
        "Go to the end after reloading",
        Kind::Bool,
    ),
    entry(
        "Files",
        "files.offer-default-app",
        "Offer to open text files at startup",
        Kind::Bool,
    ),
    entry(
        "Files",
        EXPLORER_MENU,
        "Edit with Birchpad in the Explorer's menu of files",
        Kind::ExplorerMenu,
    ),
    entry(
        "Files",
        "files.recent-submenu",
        "Recent files in a submenu",
        Kind::Bool,
    ),
    entry(
        "Files",
        "files.recent-file-name-only",
        "Recent files by their name only",
        Kind::Bool,
    ),
    entry(
        "Files",
        "files.recent-max-length",
        "Recent files: maximum length (0: no limit)",
        Kind::Number { min: 0, max: 500 },
    ),
    entry(
        "Files",
        "files.recent-check-existence",
        "Recent files: leave out those that are gone",
        Kind::Bool,
    ),
];
