//! Preferences > New Document.

use crate::preferences::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry(
        "New Document",
        "new-document.encoding",
        "Encoding",
        Kind::Choice(&[
            ("utf-8", "UTF-8"),
            ("utf-8-bom", "UTF-8 with BOM"),
            ("utf-16le-bom", "UTF-16 LE BOM"),
            ("utf-16be-bom", "UTF-16 BE BOM"),
            ("ansi", "ANSI"),
        ]),
    ),
    entry(
        "New Document",
        "new-document.line-ending",
        "Line ending",
        Kind::Choice(&[
            ("system", "The system's"),
            ("crlf", "Windows (CR LF)"),
            ("lf", "Unix (LF)"),
            ("cr", "Macintosh (CR)"),
        ]),
    ),
    entry(
        "New Document",
        "new-document.language",
        "Language (its id: python, rust, text)",
        Kind::Text,
    ),
    entry(
        "New Document",
        "new-document.apply-to-ansi",
        "Apply to opened ASCII files",
        Kind::Bool,
    ),
    entry(
        "New Document",
        "new-document.at-startup",
        "Always open a new document at startup",
        Kind::Bool,
    ),
    entry(
        "New Document",
        "new-document.first-line-name",
        "Use the first line as an untitled tab name",
        Kind::Bool,
    ),
];
