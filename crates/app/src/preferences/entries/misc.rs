//! Preferences > MISC.

use crate::preferences::{Entry, Kind, entry};

pub(super) const ENTRIES: &[Entry] = &[
    entry(
        "MISC",
        "misc.peek-on-document-map",
        "The document peeker shows in the Document Map",
        Kind::Bool,
    ),
    entry(
        "MISC",
        "misc.detect-encoding",
        "Autodetect character encoding",
        Kind::Bool,
    ),
    entry(
        "MISC",
        "misc.confirm-save-all",
        "Confirm Save All",
        Kind::Bool,
    ),
    entry(
        "MISC",
        "misc.follow-symlinks",
        "Follow symbolic links in Folder as Workspace",
        Kind::Bool,
    ),
    entry(
        "MISC",
        "misc.minimize-to-tray",
        "Minimize to system tray",
        Kind::Bool,
    ),
    entry(
        "MISC",
        "session.session-extension",
        "Session file extension",
        Kind::Text,
    ),
    entry(
        "MISC",
        "session.workspace-extension",
        "Workspace file extension",
        Kind::Text,
    ),
];
