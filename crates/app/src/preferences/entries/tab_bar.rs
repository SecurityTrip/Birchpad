//! Preferences > Tab Bar.

use crate::preferences::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry("Tab Bar", "tabs.vertical", "Vertical", Kind::Bool),
    entry("Tab Bar", "tabs.multi-line", "Multi-line", Kind::Bool),
    entry("Tab Bar", "tabs.reduce", "Reduce", Kind::Bool),
    entry(
        "Tab Bar",
        "tabs.lock",
        "Lock (no drag and drop)",
        Kind::Bool,
    ),
    entry(
        "Tab Bar",
        "tabs.close-button",
        "Show close button on each tab",
        Kind::Bool,
    ),
    entry(
        "Tab Bar",
        "tabs.double-click-close",
        "Double click to close document",
        Kind::Bool,
    ),
    entry(
        "Tab Bar",
        "tabs.peeker",
        "Show the start of a document on hover",
        Kind::Bool,
    ),
    entry(
        "Tab Bar",
        "tabs.switcher",
        "Document switcher (Ctrl+Tab)",
        Kind::Bool,
    ),
    entry(
        "Tab Bar",
        "tabs.switcher-mru",
        "Most recently used first",
        Kind::Bool,
    ),
    entry(
        "Tab Bar",
        "tabs.exit-on-last-close",
        "Exit on close the last tab",
        Kind::Bool,
    ),
    entry(
        "Tab Bar",
        "tabs.max-label-length",
        "Max tab label length (0: no limit)",
        Kind::Number { min: 0, max: 200 },
    ),
    entry(
        "Tab Bar",
        "tabs.alternate-icons",
        "Alternate icons",
        Kind::Bool,
    ),
    entry(
        "Tab Bar",
        "tabs.inactive-color",
        "Change inactive tab color",
        Kind::Bool,
    ),
    entry(
        "Tab Bar",
        "tabs.active-bar",
        "Draw a colored bar on the active tab",
        Kind::Bool,
    ),
    entry("Tab Bar", "tabs.pinning", "Enable pinning tabs", Kind::Bool),
    entry(
        "Tab Bar",
        "tabs.only-pinned-button",
        "Show the pin button only on pinned tabs",
        Kind::Bool,
    ),
    entry(
        "Tab Bar",
        "tabs.inactive-buttons",
        "Show buttons on inactive tabs",
        Kind::Bool,
    ),
];
