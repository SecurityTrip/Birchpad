//! Preferences > Print.

use crate::preferences::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry(
        "Print",
        "print.line-numbers",
        "Print line numbers",
        Kind::Bool,
    ),
    entry(
        "Print",
        "print.colors",
        "Colors",
        Kind::Choice(&[
            ("wysiwyg", "WYSIWYG"),
            ("invert", "Invert"),
            ("black-on-white", "Black on white"),
            ("no-background", "No background color"),
        ]),
    ),
    entry(
        "Print",
        "print.paper",
        "Paper",
        Kind::Choice(&[
            ("a4", "A4"),
            ("letter", "Letter"),
            ("legal", "Legal"),
            ("a3", "A3"),
            ("a5", "A5"),
        ]),
    ),
    entry("Print", "print.landscape", "Landscape", Kind::Bool),
    entry(
        "Print",
        "print.margin-top",
        "Top margin (mm)",
        Kind::Number { min: 0, max: 100 },
    ),
    entry(
        "Print",
        "print.margin-bottom",
        "Bottom margin (mm)",
        Kind::Number { min: 0, max: 100 },
    ),
    entry(
        "Print",
        "print.margin-left",
        "Left margin (mm)",
        Kind::Number { min: 0, max: 100 },
    ),
    entry(
        "Print",
        "print.margin-right",
        "Right margin (mm)",
        Kind::Number { min: 0, max: 100 },
    ),
    entry(
        "Print",
        "print.font-size",
        "Font size (pt)",
        Kind::Number { min: 4, max: 72 },
    ),
    entry("Print", "print.header-left", "Header, left", Kind::Text),
    entry("Print", "print.header-center", "Header, center", Kind::Text),
    entry("Print", "print.header-right", "Header, right", Kind::Text),
    entry("Print", "print.footer-left", "Footer, left", Kind::Text),
    entry("Print", "print.footer-center", "Footer, center", Kind::Text),
    entry("Print", "print.footer-right", "Footer, right", Kind::Text),
    entry(
        "Print",
        "print.form-feed-page-break",
        "Print a form feed as a page break",
        Kind::Bool,
    ),
];
