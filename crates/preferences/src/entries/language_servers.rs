//! Preferences > Language Servers (ADR 0065).

use crate::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry(
        "Language Servers",
        "lsp.enabled",
        "Use language servers",
        Kind::Bool,
    ),
    entry(
        "Language Servers",
        "lsp.servers",
        "Servers by language",
        Kind::LanguageServers,
    ),
];
