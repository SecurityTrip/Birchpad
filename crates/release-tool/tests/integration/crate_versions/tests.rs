use std::fs;

use birchpad_release_tool::crate_versions::*;
use semver::Version;
use std::collections::BTreeMap;
use std::path::Path;

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
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {}", args.join(" "));
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
