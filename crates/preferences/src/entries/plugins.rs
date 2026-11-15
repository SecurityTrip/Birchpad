//! Preferences > Plugins.

use crate::{Entry, Kind, PLUGIN_SETTINGS, entry};

pub(super) const ENTRIES: &[Entry] = &[entry(
    "Plugins",
    PLUGIN_SETTINGS,
    "Settings of the plugins",
    Kind::PluginSettings,
)];
