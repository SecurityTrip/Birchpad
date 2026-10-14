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

#[cfg(windows)]
fn add_at(path: &str, exe: &Path) -> io::Result<()> {
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

#[cfg(windows)]
fn remove_at(path: &str) -> io::Result<()> {
    use windows_registry::CURRENT_USER;
    if CURRENT_USER.open(path).is_err() {
        return Ok(());
    }
    CURRENT_USER.remove_tree(path).map_err(io::Error::other)
}

#[cfg(windows)]
fn is_added_at(path: &str) -> bool {
    windows_registry::CURRENT_USER
        .open(format!(r"{path}\command"))
        .and_then(|key| key.get_string(""))
        .is_ok_and(|command| !command.is_empty())
}

/// The value under [`STATE_KEY`].
#[cfg(windows)]
const REMOVED: &str = "ExplorerMenu";

#[cfg(windows)]
fn remember_removed(state: &str, removed: bool) -> io::Result<()> {
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

#[cfg(windows)]
fn was_removed(state: &str) -> bool {
    windows_registry::CURRENT_USER
        .open(state)
        .and_then(|key| key.get_string(REMOVED))
        .is_ok_and(|value| value == "removed")
}

#[cfg(not(windows))]
fn remember_removed(_: &str, _: bool) -> io::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
fn was_removed(_: &str) -> bool {
    false
}

#[cfg(not(windows))]
fn add_at(_: &str, _: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "the Explorer's menu is Windows'",
    ))
}

#[cfg(not(windows))]
fn remove_at(_: &str) -> io::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
fn is_added_at(_: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn the_command_quotes_the_program_and_the_file() {
        let exe = PathBuf::from(r"C:\Program Files\Birchpad\birchpad.exe");
        assert_eq!(
            command_line(&exe),
            r#""C:\Program Files\Birchpad\birchpad.exe" "%1""#
        );
        assert_eq!(command_line(Path::new("")), r#""" "%1""#);
    }

    /// In a key of the tests' own, not the Explorer's.
    #[cfg(windows)]
    #[test]
    fn added_changed_and_taken_away() {
        use windows_registry::CURRENT_USER;
        let root = format!(r"Software\Birchpad-tests\explorer-{}", std::process::id());
        let path = format!(r"{root}\Classes\*\shell\Birchpad");
        assert!(!is_added_at(&path));
        remove_at(&path).unwrap();

        let first = PathBuf::from(r"C:\Birchpad\birchpad.exe");
        add_at(&path, &first).unwrap();
        assert!(is_added_at(&path));
        let key = CURRENT_USER.open(&path).unwrap();
        assert_eq!(key.get_string("").unwrap(), LABEL);
        assert_eq!(
            key.get_string("Icon").unwrap(),
            r#""C:\Birchpad\birchpad.exe",0"#
        );
        // Added again from another copy: it runs that one.
        let second = PathBuf::from(r"D:\portable birchpad\birchpad.exe");
        add_at(&path, &second).unwrap();
        let command = CURRENT_USER.open(format!(r"{path}\command")).unwrap();
        assert_eq!(command.get_string("").unwrap(), command_line(&second));

        remove_at(&path).unwrap();
        assert!(!is_added_at(&path));
        remove_at(&path).unwrap();
        CURRENT_USER.remove_tree(&root).unwrap();
    }

    /// Taken away by the user: updates leave it away, until the user adds it again; uninstalling
    /// forgets it.
    #[cfg(windows)]
    #[test]
    fn a_verb_taken_away_stays_away_through_updates() {
        use windows_registry::CURRENT_USER;
        let state = format!(
            r"Software\Birchpad-tests\explorer-state-{}",
            std::process::id()
        );
        assert!(!was_removed(&state), "nothing remembered yet");
        remember_removed(&state, false).unwrap();
        assert!(!was_removed(&state), "forgetting nothing is no error");
        remember_removed(&state, true).unwrap();
        assert!(was_removed(&state));
        remember_removed(&state, true).unwrap();
        assert!(was_removed(&state), "twice is once");
        remember_removed(&state, false).unwrap();
        assert!(!was_removed(&state));
        // Another value there is not the user's choice.
        CURRENT_USER
            .create(&state)
            .unwrap()
            .set_string(REMOVED, "kept")
            .unwrap();
        assert!(!was_removed(&state));
        CURRENT_USER.remove_tree(&state).unwrap();
    }

    #[cfg(not(windows))]
    #[test]
    fn elsewhere_there_is_no_such_menu() {
        assert!(add(Path::new("/usr/bin/birchpad")).is_err());
        remove().unwrap();
        assert!(!is_added());
    }
}
