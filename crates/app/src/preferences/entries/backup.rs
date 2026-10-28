//! Preferences > Backup.

use crate::preferences::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry(
        "Backup",
        "files.backup-on-save",
        "Backup on save",
        Kind::Choice(&[
            ("none", "None"),
            ("simple", "Simple (.bak)"),
            ("verbose", "Verbose (with the time)"),
        ]),
    ),
    entry(
        "Backup",
        "files.backup-folder",
        "Folder of verbose backups",
        Kind::Text,
    ),
    entry(
        "Backup",
        "session.remember-inaccessible",
        "Remember inaccessible files",
        Kind::Bool,
    ),
    entry(
        "Backup",
        "session.remember",
        "Remember the session",
        Kind::Bool,
    ),
    entry(
        "Backup",
        "session.backup-unsaved",
        "Back up unsaved changes",
        Kind::Bool,
    ),
    entry(
        "Backup",
        "session.backup-interval-seconds",
        "Backup every (seconds)",
        Kind::Number { min: 1, max: 3600 },
    ),
];
