//! Build information for Help > About: the release channel and the commit.
//!
//! Release builds get both from the environment (`BIRCHPAD_CHANNEL`, `BIRCHPAD_COMMIT`);
//! local builds are "development" builds of the checked-out commit.

use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo::rerun-if-env-changed=BIRCHPAD_CHANNEL");
    println!("cargo::rerun-if-env-changed=BIRCHPAD_COMMIT");
    let channel = non_empty_env("BIRCHPAD_CHANNEL").unwrap_or_else(|| "development".to_owned());
    println!("cargo::rustc-env=BIRCHPAD_CHANNEL={channel}");
    if let Some(commit) = non_empty_env("BIRCHPAD_COMMIT").or_else(git_commit) {
        println!("cargo::rustc-env=BIRCHPAD_COMMIT={commit}");
    }
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    let text = text.trim();
    (output.status.success() && !text.is_empty()).then(|| text.to_owned())
}

/// The commit of the checkout, if this is one; rebuilt when HEAD moves.
fn git_commit() -> Option<String> {
    for file in ["HEAD", "logs/HEAD"] {
        if let Some(path) = git(&["rev-parse", "--git-path", file])
            && Path::new(&path).exists()
        {
            println!("cargo::rerun-if-changed={path}");
        }
    }
    git(&["rev-parse", "HEAD"])
}
