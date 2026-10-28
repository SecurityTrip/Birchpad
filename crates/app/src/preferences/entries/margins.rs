//! Preferences > Margins.

use crate::preferences::{Entry, Kind, entry};

const OFF_ON: &[(&str, &str)] = &[("off", "Off"), ("margin", "In the margin")];

pub(super) const ENTRIES: &[Entry] = &[
    entry("Margins", "editor.line-numbers", "Line numbers", Kind::Bool),
    entry("Margins", "editor.bookmark-margin", "Bookmarks", Kind::Bool),
    entry("Margins", "editor.fold-margin", "Folding", Kind::Bool),
    entry(
        "Margins",
        "editor.change-history",
        "Change history",
        Kind::Choice(OFF_ON),
    ),
    entry(
        "Margins",
        "editor.edge",
        "Vertical edge",
        Kind::Choice(&[
            ("off", "None"),
            ("line", "Line"),
            ("background", "Background"),
        ]),
    ),
    entry(
        "Margins",
        "editor.edge-columns",
        "Edge columns",
        Kind::Numbers { min: 1, max: 9999 },
    ),
    entry(
        "Margins",
        "editor.fold-style",
        "Fold margin style",
        Kind::Choice(&[
            ("simple", "Simple"),
            ("arrow", "Arrow"),
            ("circle-tree", "Circle tree"),
            ("box-tree", "Box tree"),
            ("none", "None"),
        ]),
    ),
    entry(
        "Margins",
        "editor.border-width",
        "Border width (px)",
        Kind::Number { min: 0, max: 30 },
    ),
    entry(
        "Margins",
        "editor.line-number-width",
        "Line number width",
        Kind::Choice(&[("dynamic", "Dynamic"), ("constant", "Constant")]),
    ),
    entry(
        "Margins",
        "editor.padding-left",
        "Padding left (px)",
        Kind::Number { min: 0, max: 30 },
    ),
    entry(
        "Margins",
        "editor.padding-right",
        "Padding right (px)",
        Kind::Number { min: 0, max: 30 },
    ),
    entry(
        "Margins",
        "editor.padding-distraction-free",
        "Distraction Free padding (tenths)",
        Kind::Number { min: 0, max: 9 },
    ),
];
