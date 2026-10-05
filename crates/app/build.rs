//! Build information for Help > About and updates: the version, the release channel, the commit
//! and the time of the commit. On Windows, also the executable's icon and version resources.
//!
//! Release builds get them from the environment (`BIRCHPAD_VERSION` for nightly versions, which
//! `Cargo.toml` does not carry, `BIRCHPAD_CHANNEL`, `BIRCHPAD_COMMIT`, `BIRCHPAD_BUILD_TIME`);
//! local builds are "development" builds of the checked-out commit.

use std::path::{Path, PathBuf};
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
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        windows_resources(&version);
    }
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

/// The executable's icon, which Explorer, the taskbar and shortcuts show and GPUI gives its
/// windows (it loads icon resource 1), and its version information for Explorer's Details.
fn windows_resources(version: &str) {
    let manifest_dir =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets it"));
    let icon = manifest_dir
        .join("../../packaging/icons/birchpad.ico")
        .canonicalize()
        .expect("packaging/icons/birchpad.ico exists");
    println!("cargo::rerun-if-changed={}", icon.display());
    // Windows versions are four numbers: 0.2.0-nightly.20261006.3 is 0.2.0.0.
    let numbers: Vec<u16> = version
        .split(['-', '+'])
        .next()
        .unwrap_or_default()
        .split('.')
        .map(|number| number.parse().unwrap_or(0))
        .collect();
    let [major, minor, patch] = [0, 1, 2].map(|i| numbers.get(i).copied().unwrap_or(0));
    // rc.exe takes forward slashes, and has no use for the `\\?\` prefix.
    let icon = icon.display().to_string().replace('\\', "/");
    let icon = icon.trim_start_matches("//?/");
    let rc = format!(
        r#"1 ICON "{icon}"

1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "The Birchpad Contributors"
      VALUE "FileDescription", "Birchpad"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "birchpad"
      VALUE "LegalCopyright", "MIT OR Apache-2.0"
      VALUE "OriginalFilename", "birchpad.exe"
      VALUE "ProductName", "Birchpad"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    );
    let out =
        PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets it")).join("birchpad.rc");
    std::fs::write(&out, rc).expect("cannot write the resource script");
    embed_resource::compile(&out, embed_resource::NONE)
        .manifest_optional()
        .expect("cannot compile the Windows resources (needs the Windows SDK's rc.exe)");
}
