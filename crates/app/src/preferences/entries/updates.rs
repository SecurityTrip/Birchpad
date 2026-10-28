//! Preferences > Updates.

use crate::preferences::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry(
        "Updates",
        "network.catalog",
        "Online list of plugins and dictionaries",
        Kind::Text,
    ),
    entry(
        "Updates",
        "updates.mode",
        "Updates",
        Kind::Choice(&[("off", "Off"), ("notify", "Notify"), ("auto", "Automatic")]),
    ),
    entry(
        "Updates",
        "updates.channel",
        "Channel",
        Kind::Choice(&[
            ("stable", "Stable"),
            ("beta", "Beta"),
            ("nightly", "Nightly"),
        ]),
    ),
];
