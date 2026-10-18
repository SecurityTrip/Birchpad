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
pub(crate) struct CrateVersion {
    pub(crate) name: String,
    pub(crate) version: Version,
    /// `None` for a crate the release did not have yet.
    pub(crate) previous: Option<Version>,
    /// Whether anything in its folder differs from the release.
    pub(crate) changed: bool,
}

/// What breaks the rule; empty when every version is as it should be.
pub(crate) fn problems(
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
pub(crate) fn check(repo: &Path, since: Option<&str>) -> Result<()> {
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

fn read_toml(text: &str) -> Result<toml::Table> {
    Ok(toml::from_str(text)?)
}

fn package_name(manifest: &toml::Table) -> Result<String> {
    manifest
        .get("package")
        .and_then(|package| package.get("name"))
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .context("a crate's Cargo.toml has no package name")
}

/// The crate's own version, or `None` when it takes the workspace's.
fn own_version(manifest: &toml::Table) -> Result<Option<Version>> {
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

fn workspace_version(workspace: &toml::Table) -> Result<Version> {
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
fn latest_release(repo: &Path, version: &Version) -> Result<String> {
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

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    fn krate(name: &str, now: &str, then: Option<&str>, changed: bool) -> CrateVersion {
        CrateVersion {
            name: name.to_owned(),
            version: version(now),
            previous: then.map(version),
            changed,
        }
    }

    fn deps(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(name, version)| ((*name).to_owned(), (*version).to_owned()))
            .collect()
    }

    #[test]
    fn changed_crates_rise_and_the_others_stay() {
        let crates = [
            krate("a", "0.3.0", Some("0.2.5"), true),
            krate("b", "0.2.5", Some("0.2.5"), false),
            // A patch is enough, and a crate new since the release may start anywhere.
            krate("c", "0.2.6", Some("0.2.5"), true),
            krate("d", "0.1.0", None, true),
        ];
        let deps = deps(&[
            ("a", "0.3.0"),
            ("b", "0.2.5"),
            ("c", "0.2.6"),
            ("d", "0.1.0"),
        ]);
        assert!(problems(&crates, &deps, "v0.2.5").is_empty());
        assert!(
            problems(&[], &BTreeMap::new(), "v0.2.5").is_empty(),
            "no crates at all"
        );
    }

    #[test]
    fn a_change_without_a_new_version_and_a_version_without_a_change_are_refused() {
        let crates = [
            krate("a", "0.2.5", Some("0.2.5"), true),
            krate("b", "0.3.0", Some("0.2.5"), false),
            krate("c", "0.2.4", Some("0.2.5"), true),
        ];
        let deps = deps(&[("a", "0.2.5"), ("b", "0.3.0"), ("c", "0.2.4")]);
        assert_eq!(
            problems(&crates, &deps, "v0.2.5"),
            [
                "a changed since v0.2.5 and needs a version above 0.2.5",
                "b did not change since v0.2.5 and keeps 0.2.5, not 0.3.0",
                "c 0.2.4 is below 0.2.5, its version in v0.2.5",
            ]
        );
    }

    #[test]
    fn the_workspace_depends_on_each_crate_at_its_exact_version() {
        let crates = [
            krate("a", "0.3.0", Some("0.2.5"), true),
            krate("b", "0.2.5", Some("0.2.5"), false),
        ];
        // "0.3" would accept 0.3.0 too, but says less than the crate.
        assert_eq!(
            problems(&crates, &deps(&[("a", "0.3")]), "v0.2.5"),
            [
                "the workspace depends on a 0.3, but the crate is 0.3.0",
                "the workspace's dependencies do not name b with its version",
            ]
        );
    }

    #[test]
    fn manifests_give_their_own_version_or_the_workspaces() {
        let own = read_toml("[package]\nname = \"x\"\nversion = \"1.2.3\"").unwrap();
        assert_eq!(own_version(&own).unwrap(), Some(version("1.2.3")));
        assert_eq!(package_name(&own).unwrap(), "x");
        let shared = read_toml("[package]\nname = \"x\"\nversion.workspace = true").unwrap();
        assert_eq!(own_version(&shared).unwrap(), None);
        for broken in [
            "[package]\nname = \"x\"",
            "[package]\nname = \"x\"\nversion = \"one\"",
            "[package]\nname = \"x\"\nversion = 1",
            "[package]\nname = \"x\"\nversion.workspace = false",
        ] {
            assert!(
                own_version(&read_toml(broken).unwrap()).is_err(),
                "{broken}"
            );
        }
        assert!(package_name(&read_toml("[package]").unwrap()).is_err());
        assert!(workspace_version(&read_toml("[workspace]").unwrap()).is_err());
    }

    fn run_git(repo: &Path, args: &[&str]) {
        git(repo, args).unwrap();
    }

    fn write(repo: &Path, path: &str, text: &str) {
        let path = repo.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn workspace(app: &str, lib: &str) -> String {
        format!(
            "[workspace]\nmembers = [\"crates/*\"]\n[workspace.package]\nversion = \"{app}\"\n\
             [workspace.dependencies]\nlib = {{ path = \"crates/lib\", version = \"{lib}\" }}\n"
        )
    }

    /// A repository released as v0.2.5 with the application and one library crate.
    fn repository() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        run_git(repo, &["init", "--quiet"]);
        run_git(repo, &["config", "user.email", "test@example.org"]);
        run_git(repo, &["config", "user.name", "Test"]);
        run_git(repo, &["config", "core.autocrlf", "false"]);
        write(repo, "Cargo.toml", &workspace("0.2.5", "0.2.5"));
        write(
            repo,
            "crates/app/Cargo.toml",
            "[package]\nname = \"app\"\nversion.workspace = true\n",
        );
        write(
            repo,
            "crates/lib/Cargo.toml",
            "[package]\nname = \"lib\"\nversion.workspace = true\n",
        );
        write(repo, "crates/lib/src/lib.rs", "");
        run_git(repo, &["add", "."]);
        run_git(repo, &["commit", "--quiet", "-m", "release"]);
        run_git(repo, &["tag", "v0.2.5"]);
        // A nightly build after it is no release to compare with.
        run_git(repo, &["tag", "v0.3.0-nightly.20261016.1"]);
        dir
    }

    #[test]
    fn a_repository_is_checked_against_its_latest_release() {
        let dir = repository();
        let repo = dir.path();
        let next = version("0.3.0");
        assert_eq!(latest_release(repo, &next).unwrap(), "v0.2.5");
        // The release's own tag, and versions after it, are skipped.
        run_git(repo, &["tag", "v0.3.0"]);
        run_git(repo, &["tag", "v0.4.0"]);
        assert_eq!(latest_release(repo, &next).unwrap(), "v0.2.5");
        assert!(
            latest_release(repo, &version("0.2.5")).is_err(),
            "nothing before the first"
        );
        // The next release's branch opens: the application's version rises.
        write(repo, "Cargo.toml", &workspace("0.3.0", "0.2.5"));
        // A version of its own, the one it had, is no change.
        write(
            repo,
            "crates/lib/Cargo.toml",
            "[package]\nname = \"lib\"\nversion = \"0.2.5\"\n",
        );
        check(repo, None).unwrap();
        // Its source changes, then a dependency: either way it must rise.
        write(repo, "crates/lib/src/lib.rs", "pub fn f() {}\n");
        let error = check(repo, None).unwrap_err().to_string();
        assert!(error.contains("lib changed since v0.2.5"), "{error}");
        write(repo, "crates/lib/src/lib.rs", "");
        write(
            repo,
            "crates/lib/Cargo.toml",
            "[package]\nname = \"lib\"\nversion = \"0.2.5\"\n[dependencies]\nanyhow = \"1\"\n",
        );
        let error = check(repo, None).unwrap_err().to_string();
        assert!(error.contains("lib changed since v0.2.5"), "{error}");
        // It rises, and the workspace follows.
        write(
            repo,
            "crates/lib/Cargo.toml",
            "[package]\nname = \"lib\"\nversion = \"0.3.0\"\n[dependencies]\nanyhow = \"1\"\n",
        );
        let error = check(repo, None).unwrap_err().to_string();
        assert!(
            error.contains("the workspace depends on lib 0.2.5"),
            "{error}"
        );
        write(repo, "Cargo.toml", &workspace("0.3.0", "0.3.0"));
        check(repo, None).unwrap();
        check(repo, Some("v0.2.5")).unwrap();
        assert!(
            check(repo, Some("v9.9.9")).is_err(),
            "a tag that is not there"
        );
    }

    #[test]
    fn without_a_release_there_is_nothing_to_compare_with() {
        let dir = tempfile::tempdir().unwrap();
        run_git(dir.path(), &["init", "--quiet"]);
        assert!(latest_release(dir.path(), &version("0.1.0")).is_err());
        assert!(check(dir.path(), None).is_err());
    }
}
