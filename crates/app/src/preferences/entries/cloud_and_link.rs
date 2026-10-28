//! Preferences > Cloud & Link.

use crate::preferences::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry(
        "Cloud & Link",
        "links.enabled",
        "Enable clickable links",
        Kind::Bool,
    ),
    entry(
        "Cloud & Link",
        "links.double-click",
        "Open them with a double click too",
        Kind::Bool,
    ),
    entry(
        "Cloud & Link",
        "links.underline",
        "Underline them",
        Kind::Bool,
    ),
    entry(
        "Cloud & Link",
        "links.schemes",
        "Schemes of one's own",
        Kind::Text,
    ),
    entry(
        "Cloud & Link",
        "cloud.folder",
        "Settings in this folder (from the next start)",
        Kind::Text,
    ),
    entry(
        "Cloud & Link",
        "links.hover-box",
        "Frame the link under the mouse",
        Kind::Bool,
    ),
];
