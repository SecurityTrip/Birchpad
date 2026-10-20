//! "Edit with Birchpad" in the Windows Explorer's context menu of files, as Notepad++'s
//! installer adds it: a verb under `Software\Classes\*\shell` that opens the file in Birchpad.
//! The per-user installer adds it for the user when Birchpad is installed and again with every
//! update, unless the user took it away in Preferences (which is remembered), and takes it away
//! when Birchpad is uninstalled; the MSI adds it for the machine with registry keys of its own
//! (`Birchpad.wxs`); any copy, the portable one too, adds it for the user or takes it away in
//! Preferences > Files.

use std::io;
use std::path::Path;

/// Where the verb is, under the user's registry (`HKEY_CURRENT_USER`).
pub const VERB_KEY: &str = r"Software\Classes\*\shell\Birchpad";

/// What the Explorer shows. English, as Notepad++'s "Edit with Notepad++".
pub const LABEL: &str = "Edit with Birchpad";

/// Where Birchpad remembers that the user took the verb away (`ExplorerMenu = "removed"`), so
/// that updates leave it away.
pub const STATE_KEY: &str = r"Software\Birchpad";

/// The command the Explorer runs for a file: the program and the file, both quoted.
pub fn command_line(exe: &Path) -> String {
    format!("\"{}\" \"%1\"", exe.display())
}

/// Adds the verb for the current user, running `exe`; again, it runs `exe` from then on.
pub fn add(exe: &Path) -> io::Result<()> {
    add_at(VERB_KEY, exe)?;
    remember_removed(STATE_KEY, false)
}

/// Takes the verb away for the current user, and remembers it; nothing to take away is no
/// error.
pub fn remove() -> io::Result<()> {
    remove_at(VERB_KEY)?;
    remember_removed(STATE_KEY, true)
}

/// After an update: adds the verb, unless the user took it away.
pub fn add_unless_removed(exe: &Path) -> io::Result<()> {
    if was_removed(STATE_KEY) {
        return Ok(());
    }
    add_at(VERB_KEY, exe)
}

/// Before uninstalling: takes the verb away and forgets that it was ever taken away.
pub fn remove_for_uninstall() -> io::Result<()> {
    remove_at(VERB_KEY)?;
    remember_removed(STATE_KEY, false)
}

/// Whether the user took the verb away, so that updates leave it away.
pub fn is_removed_by_user() -> bool {
    was_removed(STATE_KEY)
}

/// Whether the current user has the verb.
pub fn is_added() -> bool {
    is_added_at(VERB_KEY)
}

/// Adds the verb at `path` of the user's registry ([`VERB_KEY`] is the Explorer's), running
/// `exe`.
#[cfg(windows)]
pub fn add_at(path: &str, exe: &Path) -> io::Result<()> {
    use windows_registry::CURRENT_USER;
    let key = CURRENT_USER.create(path).map_err(io::Error::other)?;
    key.set_string("", LABEL).map_err(io::Error::other)?;
    key.set_string("Icon", format!("\"{}\",0", exe.display()))
        .map_err(io::Error::other)?;
    let command = key.create("command").map_err(io::Error::other)?;
    command
        .set_string("", command_line(exe))
        .map_err(io::Error::other)
}

/// Takes the verb at `path` away; nothing there is no error.
#[cfg(windows)]
pub fn remove_at(path: &str) -> io::Result<()> {
    use windows_registry::CURRENT_USER;
    if CURRENT_USER.open(path).is_err() {
        return Ok(());
    }
    CURRENT_USER.remove_tree(path).map_err(io::Error::other)
}

/// Whether the verb at `path` is there, with a command.
#[cfg(windows)]
pub fn is_added_at(path: &str) -> bool {
    windows_registry::CURRENT_USER
        .open(format!(r"{path}\command"))
        .and_then(|key| key.get_string(""))
        .is_ok_and(|command| !command.is_empty())
}

/// The value under [`STATE_KEY`] that remembers the user took the verb away.
pub const REMOVED: &str = "ExplorerMenu";

/// Remembers under `state` ([`STATE_KEY`] is Birchpad's) whether the user took the verb away.
#[cfg(windows)]
pub fn remember_removed(state: &str, removed: bool) -> io::Result<()> {
    use windows_registry::CURRENT_USER;
    if removed {
        return CURRENT_USER
            .create(state)
            .and_then(|key| key.set_string(REMOVED, "removed"))
            .map_err(io::Error::other);
    }
    match CURRENT_USER.open(state) {
        Ok(key) if key.get_string(REMOVED).is_ok() => CURRENT_USER
            .create(state)
            .and_then(|key| key.remove_value(REMOVED))
            .map_err(io::Error::other),
        _ => Ok(()),
    }
}

/// Whether `state` remembers that the user took the verb away.
#[cfg(windows)]
pub fn was_removed(state: &str) -> bool {
    windows_registry::CURRENT_USER
        .open(state)
        .and_then(|key| key.get_string(REMOVED))
        .is_ok_and(|value| value == "removed")
}

#[cfg(not(windows))]
pub fn remember_removed(_: &str, _: bool) -> io::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
pub fn was_removed(_: &str) -> bool {
    false
}

#[cfg(not(windows))]
pub fn add_at(_: &str, _: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "the Explorer's menu is Windows'",
    ))
}

#[cfg(not(windows))]
pub fn remove_at(_: &str) -> io::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
pub fn is_added_at(_: &str) -> bool {
    false
}
