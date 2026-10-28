//! Preferences > Preview.

use crate::preferences::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry(
        "Preview",
        "preview.plantuml",
        "PlantUML (command or plantuml.jar)",
        Kind::Text,
    ),
    entry(
        "Preview",
        "preview.plantuml-background",
        "Diagram background",
        Kind::Choice(&[("white", "White"), ("theme", "As the theme")]),
    ),
    entry(
        "Preview",
        "preview.plantuml-png-scale",
        "Diagrams copied or exported as PNG: scale",
        Kind::Number { min: 1, max: 8 },
    ),
];
