use birchpad_io::*;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::atomic::AtomicBool;

fn takes(filters: &str, name: &str) -> bool {
    Filters::parse(filters).unwrap().takes_file(name)
}

#[test]
fn wildcards_match_whole_names_without_regard_to_case() {
    assert!(takes("*.rs", "main.rs"));
    assert!(takes("*.rs", "MAIN.RS"));
    assert!(takes("журнал.*", "ЖУРНАЛ.txt"));
    assert!(takes("a?c", "abc"));
    assert!(takes("*a*b*", "xxaxxbxx"));
    assert!(takes("**.rs", "x.rs"), "two stars are one");
    // Not a part of the name only, and `?` is exactly one character.
    assert!(!takes("*.rs", "main.rsx"));
    assert!(!takes("main", "main.rs"));
    assert!(!takes("a?c", "ac"));
    assert!(!takes("a?c", "abbc"));
    assert!(!takes("*a*b", "xxaxxbx"));
}

#[test]
fn every_file_without_patterns_and_with_the_catch_alls() {
    for filters in ["", "   ", ";", "*", "*.*"] {
        for name in ["Makefile", "main.rs", ".hidden", "a.b.c"] {
            assert!(takes(filters, name), "{filters:?} takes {name}");
        }
    }
    // At the edges of a name: the empty name, one character.
    assert!(takes("*", ""));
    assert!(!takes("?", ""));
    assert!(takes("?", "x"));
    assert!(!takes("x", ""));
}

#[test]
fn exclusions_win_over_inclusions() {
    assert!(takes("*.rs;*.toml", "Cargo.toml"));
    assert!(takes("*.rs  *.toml", "lib.rs"), "several spaces");
    assert!(!takes("*.rs *.toml", "README.md"));
    assert!(!takes("*.rs !*_test.rs", "parse_test.rs"));
    assert!(takes("*.rs !*_test.rs", "parse.rs"));
    // An exclusion alone takes every other file.
    assert!(takes("!*.bak", "notes.txt"));
    assert!(!takes("!*.bak", "notes.BAK"));
    assert!(!takes("*.* !*", "anything"), "everything left out");
}

#[test]
fn folders_are_left_out_at_the_top_or_anywhere() {
    let filters = Filters::parse(r"*.rs !\target !+/node_modules !+\.g?t").unwrap();
    assert!(filters.leaves_out_folder("target", true));
    assert!(
        !filters.leaves_out_folder("target", false),
        "only at the top"
    );
    assert!(filters.leaves_out_folder("TARGET", true));
    assert!(filters.leaves_out_folder("node_modules", true));
    assert!(filters.leaves_out_folder("node_modules", false));
    assert!(filters.leaves_out_folder(".git", false));
    assert!(!filters.leaves_out_folder("src", true));
    // Folder exclusions say nothing about files of that name.
    assert!(!filters.takes_file("target"));
    assert!(Filters::parse(r"!\target").unwrap().takes_file("target"));
}

#[test]
fn exclusions_without_a_name_are_refused() {
    for bad in ["!", r"!\", "!/", "!+", r"!+\", "!+name", "*.rs !"] {
        let error = Filters::parse(bad).unwrap_err();
        assert!(
            error.to_string().contains("leaves out nothing"),
            "{bad}: {error}"
        );
    }
    // The shortest that are fine.
    assert!(Filters::parse("!a").is_ok());
    assert!(Filters::parse(r"!\a").is_ok());
    assert!(Filters::parse(r"!+\a").is_ok());
}

/// A tree of empty files and folders; paths ending with `/` are folders.
fn tree(paths: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for path in paths {
        let full = dir.path().join(path);
        if path.ends_with('/') {
            fs::create_dir_all(full).unwrap();
        } else {
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(full, "").unwrap();
        }
    }
    dir
}

fn list(root: &Path, filters: &str, subfolders: bool, hidden: bool) -> Vec<String> {
    let options = FolderOptions { subfolders, hidden };
    let filters = Filters::parse(filters).unwrap();
    files_in(root, &filters, options, &AtomicBool::new(false))
        .unwrap()
        .into_iter()
        .map(|path| {
            let relative = path.strip_prefix(root).unwrap();
            relative.to_string_lossy().replace('\\', "/")
        })
        .collect()
}

#[test]
fn files_come_before_subfolders_each_in_name_order() {
    let dir = tree(&[
        "b.txt",
        "A.txt",
        "z/inner.txt",
        "a/deeper/last.txt",
        "a/first.txt",
        "c.rs",
        "empty/",
    ]);
    assert_eq!(
        list(dir.path(), "", true, false),
        [
            "A.txt",
            "b.txt",
            "c.rs",
            "a/first.txt",
            "a/deeper/last.txt",
            "z/inner.txt"
        ]
    );
    assert_eq!(
        list(dir.path(), "*.txt", false, false),
        ["A.txt", "b.txt"],
        "without subfolders"
    );
    assert_eq!(list(dir.path(), "*.md", true, false), Vec::<String>::new());
}

#[test]
fn left_out_folders_are_not_entered() {
    let dir = tree(&[
        "target/a.rs",
        "src/target/b.rs",
        "src/node_modules/c.rs",
        "node_modules/d.rs",
        "e.rs",
    ]);
    assert_eq!(
        list(dir.path(), r"*.rs !\target !+\node_modules", true, false),
        ["e.rs", "src/target/b.rs"]
    );
}

#[test]
fn hidden_files_and_folders_only_when_asked() {
    let dir = tree(&[".hidden/a.txt", ".secret.txt", "shown.txt"]);
    #[cfg(windows)]
    for hidden in [".hidden", ".secret.txt"] {
        let status = std::process::Command::new("attrib")
            .arg("+h")
            .arg(dir.path().join(hidden))
            .status()
            .unwrap();
        assert!(status.success());
    }
    assert_eq!(list(dir.path(), "", true, false), ["shown.txt"]);
    assert_eq!(
        list(dir.path(), "", true, true),
        [".secret.txt", "shown.txt", ".hidden/a.txt"]
    );
}

#[test]
fn a_root_that_is_no_folder_is_an_error() {
    let dir = tree(&["file.txt", "empty/"]);
    let options = FolderOptions {
        subfolders: true,
        hidden: false,
    };
    let filters = Filters::default();
    let never = AtomicBool::new(false);
    let missing = files_in(&dir.path().join("missing"), &filters, options, &never);
    assert_eq!(missing.unwrap_err().kind(), io::ErrorKind::NotFound);
    let file = files_in(&dir.path().join("file.txt"), &filters, options, &never);
    assert_eq!(file.unwrap_err().kind(), io::ErrorKind::NotADirectory);
    // An empty folder is no error, only no files.
    assert!(
        files_in(&dir.path().join("empty"), &filters, options, &never)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn cancelling_stops_the_listing() {
    let dir = tree(&["a.txt", "sub/b.txt"]);
    let options = FolderOptions {
        subfolders: true,
        hidden: false,
    };
    let cancelled = AtomicBool::new(true);
    let files = files_in(dir.path(), &Filters::default(), options, &cancelled).unwrap();
    assert!(files.is_empty());
}

#[cfg(unix)]
#[test]
fn links_to_folders_are_not_followed() {
    let dir = tree(&["sub/a.txt"]);
    std::os::unix::fs::symlink(dir.path(), dir.path().join("sub/loop")).unwrap();
    std::os::unix::fs::symlink(dir.path().join("sub/a.txt"), dir.path().join("link.txt")).unwrap();
    assert_eq!(list(dir.path(), "", true, false), ["link.txt", "sub/a.txt"]);
}
