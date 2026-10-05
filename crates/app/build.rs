//! Build information for Help > About and updates: the version, the release channel, the commit
//! and the time of the commit.
//!
//! Release builds get them from the environment (`BIRCHPAD_VERSION` for nightly versions, which
//! `Cargo.toml` does not carry, `BIRCHPAD_CHANNEL`, `BIRCHPAD_COMMIT`, `BIRCHPAD_BUILD_TIME`);
//! local builds are "development" builds of the checked-out commit.

use std::path::Path;
use std::process::Command;

fn main() {
    for name in [
        "BIRCHPAD_VERSION",
        "BIRCHPAD_CHANNEL",
        "BIRCHPAD_COMMIT",
        "BIRCHPAD_BUILD_TIME",
    ] {
        println!("cargo::rerun-if-env-changed={name}");
    }
    let version = non_empty_env("BIRCHPAD_VERSION")
        .unwrap_or_else(|| std::env::var("CARGO_PKG_VERSION").expect("cargo sets the version"));
    println!("cargo::rustc-env=BIRCHPAD_VERSION={version}");
    let channel = non_empty_env("BIRCHPAD_CHANNEL").unwrap_or_else(|| "development".to_owned());
    println!("cargo::rustc-env=BIRCHPAD_CHANNEL={channel}");
    if let Some(commit) = non_empty_env("BIRCHPAD_COMMIT").or_else(git_commit) {
        println!("cargo::rustc-env=BIRCHPAD_COMMIT={commit}");
    }
    // Update manifests signed before a release was built are older than the one that announced
    // it. A development build of a newer commit is announced by none, so it has no such bound.
    let time = non_empty_env("BIRCHPAD_BUILD_TIME")
        .or_else(|| {
            (channel != "development")
                .then(|| git(&["log", "-1", "--format=%ct"]))
                .flatten()
        })
        .and_then(|time| time.trim().parse::<u64>().ok())
        .unwrap_or(0);
    println!("cargo::rustc-env=BIRCHPAD_BUILD_TIME={time}");
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
