//! Preferences > Multi-Instance & Date.

use crate::preferences::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry(
        "Multi-Instance & Date",
        "instances.mode",
        "Instances",
        Kind::Choice(&[
            ("single", "Default (mono-instance)"),
            ("session", "Open session in a new instance"),
            ("multi", "Always in multi-instance mode"),
        ]),
    ),
    entry(
        "Multi-Instance & Date",
        "date.custom-format",
        "Customized date and time",
        Kind::Text,
    ),
    entry(
        "Multi-Instance & Date",
        "date.reverse",
        "Date before time (short and long)",
        Kind::Bool,
    ),
    entry(
        "Multi-Instance & Date",
        "instances.nosession-panels",
        "Remember panels with -nosession",
        Kind::Bool,
    ),
];
