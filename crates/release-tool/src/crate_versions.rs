//! `birchpad-release crate-versions [--since TAG] [--repo DIR]`: a library crate's version
//! rises in a release only if the crate changed in it.
//!
//! The application (`birchpad`) and the tools that ship with it (`birchpad-fuzz`,
//! `birchpad-release-tool`) take the workspace version, the version of the release. Every other
//! crate under `crates/` has a version of its own: one whose folder changed since `TAG` (the
//! latest stable release by default) must be at a version above the one it had there, and one
//! that did not change must keep it. The workspace's dependency on each crate names its exact
//! version.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use anyhow::{Context as _, Result, bail, ensure};
use semver::Version;

/// One library crate: its version now and at the release compared with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrateVersion {
    pub name: String,
    pub version: Version,
    /// `None` for a crate the release did not have yet.
    pub previous: Option<Version>,
    /// Whether anything in its folder differs from the release.
    pub changed: bool,
}

/// What breaks the rule; empty when every version is as it should be.
pub fn problems(
    crates: &[CrateVersion],
    dependencies: &BTreeMap<String, String>,
    since: &str,
) -> Vec<String> {
    let mut found = Vec::new();
    for krate in crates {
        let (name, version) = (&krate.name, &krate.version);
        if let Some(previous) = &krate.previous {
            if version < previous {
                found.push(format!(
                    "{name} {version} is below {previous}, its version in {since}"
                ));
            } else if krate.changed && version == previous {
                found.push(format!(
                    "{name} changed since {since} and needs a version above {previous}"
                ));
            } else if !krate.changed && version != previous {
                found.push(format!(
                    "{name} did not change since {since} and keeps {previous}, not {version}"
                ));
            }
        }
        match dependencies.get(name) {
            Some(required) if *required == version.to_string() => {}
            Some(required) => found.push(format!(
                "the workspace depends on {name} {required}, but the crate is {version}"
            )),
            None => found.push(format!(
                "the workspace's dependencies do not name {name} with its version"
            )),
        }
    }
    found
}

/// Runs the check in the repository `repo` against the tag `since`, or the latest stable
/// release's tag.
pub fn check(repo: &Path, since: Option<&str>) -> Result<()> {
    let workspace = read_toml(&std::fs::read_to_string(repo.join("Cargo.toml"))?)?;
    let since = match since {
        Some(tag) => tag.to_owned(),
        None => latest_release(repo, &workspace_version(&workspace)?)?,
    };
    let workspace_then = read_toml(&git(repo, &["show", &format!("{since}:Cargo.toml")])?)?;
    let release = workspace_version(&workspace_then)?;
    let mut crates = Vec::new();
    let mut folders: Vec<_> = std::fs::read_dir(repo.join("crates"))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.join("Cargo.toml").is_file())
        .collect();
    folders.sort();
    for folder in folders {
        let dir = folder.file_name().unwrap_or_default().to_string_lossy();
        let manifest = read_toml(&std::fs::read_to_string(folder.join("Cargo.toml"))?)?;
        let Some(version) = own_version(&manifest)? else {
            continue;
        };
        let name = package_name(&manifest)?;
        let then = match git(repo, &["show", &format!("{since}:crates/{dir}/Cargo.toml")]) {
            Ok(text) => Some(read_toml(&text)?),
            Err(_) => None,
        };
        let previous = match &then {
            Some(then) => Some(own_version(then)?.unwrap_or_else(|| release.clone())),
            None => None,
        };
        // The version itself is no change: the manifest without it, and every other file.
        let files_changed = !git_quiet(
            repo,
            &[
                "diff",
                "--quiet",
                &since,
                "--",
                &format!("crates/{dir}"),
                &format!(":(exclude)crates/{dir}/Cargo.toml"),
            ],
        )?;
        let changed = files_changed
            || then.is_none_or(|then| without_version(then) != without_version(manifest.clone()));
        crates.push(CrateVersion {
            name,
            version,
            previous,
            changed,
        });
    }
    let found = problems(&crates, &dependency_versions(&workspace), &since);
    for krate in &crates {
        let state = if krate.changed {
            "changed"
        } else {
            "unchanged"
        };
        println!("{} {} ({state})", krate.name, krate.version);
    }
    ensure!(found.is_empty(), "crate versions:\n{}", found.join("\n"));
    Ok(())
}

/// A manifest's text as a table.
pub fn read_toml(text: &str) -> Result<toml::Table> {
    Ok(toml::from_str(text)?)
}

/// The `[package]`'s name.
pub fn package_name(manifest: &toml::Table) -> Result<String> {
    manifest
        .get("package")
        .and_then(|package| package.get("name"))
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .context("a crate's Cargo.toml has no package name")
}

/// The crate's own version, or `None` when it takes the workspace's.
pub fn own_version(manifest: &toml::Table) -> Result<Option<Version>> {
    let version = manifest
        .get("package")
        .and_then(|package| package.get("version"))
        .context("a crate's Cargo.toml has no version")?;
    match version {
        toml::Value::String(text) => Ok(Some(Version::parse(text)?)),
        toml::Value::Table(table)
            if table.get("workspace") == Some(&toml::Value::Boolean(true)) =>
        {
            Ok(None)
        }
        other => bail!("a crate's version is neither text nor the workspace's: {other}"),
    }
}

/// The manifest without its package's version, to compare what else it says.
fn without_version(mut manifest: toml::Table) -> toml::Table {
    if let Some(toml::Value::Table(package)) = manifest.get_mut("package") {
        package.remove("version");
    }
    manifest
}

/// The `[workspace.package]`'s version: the application's.
pub fn workspace_version(workspace: &toml::Table) -> Result<Version> {
    let text = workspace
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .context("the workspace has no version")?;
    Ok(Version::parse(text)?)
}

/// `[workspace.dependencies]`: the version each workspace crate is depended on with.
fn dependency_versions(workspace: &toml::Table) -> BTreeMap<String, String> {
    let Some(dependencies) = workspace
        .get("workspace")
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(toml::Value::as_table)
    else {
        return BTreeMap::new();
    };
    dependencies
        .iter()
        .filter_map(|(name, dependency)| {
            let table = dependency.as_table()?;
            table.get("path")?;
            let version = table.get("version")?.as_str()?;
            Some((name.clone(), version.to_owned()))
        })
        .collect()
}

/// The tag of the latest stable release before `version`, the one being made: `v` and a
/// version without a pre-release part. The release's own tag, once it is there, is not it.
pub fn latest_release(repo: &Path, version: &Version) -> Result<String> {
    let tags = git(repo, &["tag", "--list", "v*"])?;
    tags.lines()
        .filter_map(|tag| Some((Version::parse(tag.strip_prefix('v')?).ok()?, tag)))
        .filter(|(tagged, _)| tagged.pre.is_empty() && tagged < version)
        .max()
        .map(|(_, tag)| tag.to_owned())
        .context("no release tag (v1.2.3) to compare with; fetch the tags or pass --since")
}

fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .context("cannot run git")?;
    ensure!(
        output.status.success(),
        "git {}: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8(output.stdout)?)
}

/// Runs a git command that answers with its exit code: `true` for 0, `false` for 1.
fn git_quiet(repo: &Path, args: &[&str]) -> Result<bool> {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .status()
        .context("cannot run git")?;
    match status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => bail!("git {} failed", args.join(" ")),
    }
}
