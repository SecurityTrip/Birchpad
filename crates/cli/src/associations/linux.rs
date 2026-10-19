use std::io;
use std::path::Path;
use std::process::Command;

use super::{DESKTOP_FILE, MIME_TYPES, is_birchpad_desktop};

pub(super) const ROOT: &str = "";

pub(super) fn register(_: &str, _: &Path) -> io::Result<()> {
    run("xdg-mime", &register_args())
}

pub(super) fn unregister(_: &str) -> io::Result<()> {
    Ok(())
}

pub(super) fn is_default(_: &str) -> bool {
    answers_birchpad("xdg-mime", &["query", "default", "text/plain"])
}

/// `xdg-mime default`'s arguments: the desktop file, then the MIME types.
fn register_args() -> Vec<&'static str> {
    let mut args = vec!["default", DESKTOP_FILE];
    args.extend(MIME_TYPES);
    args
}

/// Runs `program`; an error if it cannot start or does not succeed.
fn run(program: &str, args: &[&str]) -> io::Result<()> {
    let status = Command::new(program).args(args).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("{program}: {status}")))
    }
}

/// Whether `program` answers Birchpad's desktop file; not if it cannot start.
fn answers_birchpad(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .output()
        .is_ok_and(|output| is_birchpad_desktop(&String::from_utf8_lossy(&output.stdout)))
}

/// With programs of every system instead of `xdg-mime`, which would change the user's
/// defaults.
#[cfg(test)]
#[path = "../../tests/unit/associations/imp/tests.rs"]
mod tests;
