//! Preferences > Search Engine.

use crate::preferences::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry(
        "Search Engine",
        "search-engine.engine",
        "Search on Internet with",
        Kind::Choice(&[
            ("duckduckgo", "DuckDuckGo"),
            ("google", "Google"),
            ("bing", "Bing"),
            ("yahoo", "Yahoo!"),
            ("stack-overflow", "Stack Overflow"),
            ("custom", "Custom"),
        ]),
    ),
    entry(
        "Search Engine",
        "search-engine.custom-url",
        "Custom address ($(CURRENT_WORD))",
        Kind::Text,
    ),
];
