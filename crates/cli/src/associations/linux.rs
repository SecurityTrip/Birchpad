use std::io;
use std::path::Path;
use std::process::Command;

use super::{is_birchpad_desktop, xdg_mime_default_args};

pub(super) const ROOT: &str = "";

pub(super) fn register(_: &str, _: &Path) -> io::Result<()> {
    run("xdg-mime", &xdg_mime_default_args())
}

pub(super) fn unregister(_: &str) -> io::Result<()> {
    Ok(())
}

pub(super) fn is_default(_: &str) -> bool {
    answers_birchpad("xdg-mime", &["query", "default", "text/plain"])
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
