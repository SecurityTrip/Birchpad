use std::fs;
use std::path::{Path, PathBuf};

use birchpad_io::*;

fn tree(paths: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for path in paths {
        let full = dir.path().join(path);
        if path.ends_with('/') {
            fs::create_dir_all(full).unwrap();
        } else {
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(full, path.as_bytes()).unwrap();
        }
    }
    dir
}

fn names(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn names_of_files_and_folders() {
    for good in [
        "a",
        "a.txt",
        ".gitignore",
        "имя файла.md",
        "a..b",
        "...",
        "x".repeat(255).as_str(),
    ] {
        assert!(check_name(good).is_ok(), "{good}");
    }
    for empty in ["", " ", "\t"] {
        assert!(
            matches!(check_name(empty), Err(EntryError::EmptyName)),
            "{empty:?}"
        );
    }
    for bad in ["a/b", "a\\b", ".", "..", "/", "a\0b"] {
        assert!(
            matches!(check_name(bad), Err(EntryError::NotAName(_))),
            "{bad:?}"
        );
    }
    assert_eq!(
        check_name("a:b").is_err(),
        cfg!(windows),
        "Windows refuses a colon"
    );
    assert_eq!(check_name("what?").is_err(), cfg!(windows));
}

#[test]
fn new_files_and_folders() {
    let dir = tree(&["taken.txt", "sub/"]);
    let file = create_file(dir.path(), "new.txt").unwrap();
    assert_eq!(file, dir.path().join("new.txt"));
    assert_eq!(fs::read(&file).unwrap(), b"", "empty");
    let folder = create_folder(dir.path(), "made").unwrap();
    assert!(folder.is_dir());
    // In a new folder, and with a name of one character.
    assert!(create_file(&folder, "x").unwrap().is_file());

    // A name taken is refused, and the file keeps its text.
    assert!(matches!(
        create_file(dir.path(), "taken.txt"),
        Err(EntryError::AlreadyExists(path)) if path == dir.path().join("taken.txt")
    ));
    assert_eq!(
        fs::read_to_string(dir.path().join("taken.txt")).unwrap(),
        "taken.txt"
    );
    assert!(matches!(
        create_folder(dir.path(), "sub"),
        Err(EntryError::AlreadyExists(_))
    ));
    assert!(matches!(
        create_file(dir.path(), "sub"),
        Err(EntryError::AlreadyExists(_))
    ));
    // Bad names, and a folder that is not there.
    assert!(matches!(
        create_file(dir.path(), ""),
        Err(EntryError::EmptyName)
    ));
    assert!(matches!(
        create_folder(dir.path(), "a/b"),
        Err(EntryError::NotAName(_))
    ));
    assert!(matches!(
        create_file(&dir.path().join("missing"), "a.txt"),
        Err(EntryError::Io { .. })
    ));
    assert_eq!(names(dir.path()), ["made", "new.txt", "sub", "taken.txt"]);
}

#[test]
fn renaming() {
    let dir = tree(&["a.txt", "b.txt", "folder/inner.txt"]);
    let renamed = rename(&dir.path().join("a.txt"), "c.txt").unwrap();
    assert_eq!(renamed, dir.path().join("c.txt"));
    assert_eq!(fs::read_to_string(&renamed).unwrap(), "a.txt");
    // A folder keeps what is in it.
    let folder = rename(&dir.path().join("folder"), "renamed").unwrap();
    assert!(folder.join("inner.txt").is_file());
    // The same name changes nothing.
    assert_eq!(rename(&renamed, "c.txt").unwrap(), renamed);

    // A name taken is refused; nothing moves.
    assert!(matches!(
        rename(&renamed, "b.txt"),
        Err(EntryError::AlreadyExists(_))
    ));
    assert_eq!(
        fs::read_to_string(dir.path().join("b.txt")).unwrap(),
        "b.txt"
    );
    assert!(matches!(
        rename(&renamed, ".."),
        Err(EntryError::NotAName(_))
    ));
    assert!(matches!(rename(&renamed, " "), Err(EntryError::EmptyName)));
    assert!(matches!(
        rename(&dir.path().join("gone.txt"), "x.txt"),
        Err(EntryError::Io { .. })
    ));
    assert_eq!(names(dir.path()), ["b.txt", "c.txt", "renamed"]);
}

#[test]
fn renaming_only_the_case() {
    let dir = tree(&["name.txt"]);
    let renamed = rename(&dir.path().join("name.txt"), "Name.txt").unwrap();
    assert_eq!(renamed, dir.path().join("Name.txt"));
    assert_eq!(names(dir.path()), ["Name.txt"]);
}

#[test]
fn free_names_for_copies() {
    let dir = tree(&[
        "a.txt",
        "a copy.txt",
        ".gitignore",
        "src/",
        "archive.tar.gz",
        "README",
    ]);
    let free = |name: &str| free_name(dir.path(), name);
    assert_eq!(
        free("new.txt"),
        dir.path().join("new.txt"),
        "a free name stays"
    );
    assert_eq!(free("a.txt"), dir.path().join("a copy 2.txt"));
    assert_eq!(free(".gitignore"), dir.path().join(".gitignore copy"));
    assert_eq!(free("src"), dir.path().join("src copy"));
    assert_eq!(
        free("archive.tar.gz"),
        dir.path().join("archive.tar copy.gz")
    );
    assert_eq!(free("README"), dir.path().join("README copy"));
    fs::write(dir.path().join("a copy 2.txt"), "").unwrap();
    assert_eq!(free("a.txt"), dir.path().join("a copy 3.txt"));
}

#[test]
fn copying_files_and_folders() {
    let dir = tree(&[
        "a.txt",
        "src/main.rs",
        "src/deep/lib.rs",
        "src/empty/",
        "into/",
    ]);
    let into = dir.path().join("into");
    let copied = copy_into(&dir.path().join("a.txt"), &into).unwrap();
    assert_eq!(copied, into.join("a.txt"), "its own name where it is free");
    assert_eq!(fs::read_to_string(&copied).unwrap(), "a.txt");
    assert!(dir.path().join("a.txt").exists(), "the original stays");
    // Again: a name of its own.
    assert_eq!(
        copy_into(&dir.path().join("a.txt"), &into).unwrap(),
        into.join("a copy.txt")
    );

    // A folder, with all that is in it, empty folders too.
    let src = copy_into(&dir.path().join("src"), &into).unwrap();
    assert_eq!(
        fs::read_to_string(src.join("deep").join("lib.rs")).unwrap(),
        "src/deep/lib.rs"
    );
    assert!(src.join("empty").is_dir());

    // Duplicating: beside itself.
    assert_eq!(
        duplicate(&dir.path().join("a.txt")).unwrap(),
        dir.path().join("a copy.txt")
    );
    assert_eq!(
        duplicate(&dir.path().join("src")).unwrap(),
        dir.path().join("src copy")
    );
}

#[test]
fn copies_that_cannot_be_made() {
    let dir = tree(&["src/inner/", "a.txt"]);
    let src = dir.path().join("src");
    assert!(matches!(
        copy_into(&src, &src),
        Err(EntryError::IntoItself(_))
    ));
    assert!(matches!(
        copy_into(&src, &src.join("inner")),
        Err(EntryError::IntoItself(_))
    ));
    assert!(matches!(
        copy_into(&dir.path().join("gone.txt"), dir.path()),
        Err(EntryError::Io { .. })
    ));
    assert!(matches!(
        copy_into(&dir.path().join("a.txt"), &dir.path().join("missing")),
        Err(EntryError::Io { .. })
    ));
    assert_eq!(names(dir.path()), ["a.txt", "src"]);
    assert_eq!(names(&src), ["inner"]);
}

#[test]
fn moving() {
    let dir = tree(&["a.txt", "src/main.rs", "to/", "to/taken.txt", "taken.txt"]);
    let to = dir.path().join("to");
    let moved = move_into(&dir.path().join("a.txt"), &to).unwrap();
    assert_eq!(moved, to.join("a.txt"));
    assert!(!dir.path().join("a.txt").exists());
    let src = move_into(&dir.path().join("src"), &to).unwrap();
    assert!(src.join("main.rs").is_file());
    // Into the folder it is in: it stays.
    assert_eq!(move_into(&moved, &to).unwrap(), moved);

    // A name taken there is refused; both stay.
    assert!(matches!(
        move_into(&dir.path().join("taken.txt"), &to),
        Err(EntryError::AlreadyExists(path)) if path == to.join("taken.txt")
    ));
    assert_eq!(
        fs::read_to_string(to.join("taken.txt")).unwrap(),
        "to/taken.txt"
    );
    assert!(dir.path().join("taken.txt").exists());
    // Not into itself, nor from nowhere.
    assert!(matches!(
        move_into(&to, &to),
        Err(EntryError::IntoItself(_))
    ));
    assert!(matches!(
        move_into(&to, &src),
        Err(EntryError::IntoItself(_))
    ));
    assert!(matches!(
        move_into(&dir.path().join("gone"), &to),
        Err(EntryError::Io { .. })
    ));
    assert_eq!(names(&to), ["a.txt", "src", "taken.txt"]);
}

#[test]
fn paths_follow_what_moved() {
    let from = PathBuf::from("root").join("old");
    let to = PathBuf::from("root").join("new");
    assert_eq!(relocated(&from, &from, &to), Some(to.clone()));
    assert_eq!(
        relocated(&from.join("a").join("b.txt"), &from, &to),
        Some(to.join("a").join("b.txt"))
    );
    // A sibling whose name starts the same is not inside it.
    assert_eq!(
        relocated(&PathBuf::from("root").join("older"), &from, &to),
        None
    );
    assert_eq!(relocated(Path::new("root"), &from, &to), None);
    assert_eq!(relocated(Path::new(""), &from, &to), None);
}

#[test]
fn errors_read_as_sentences() {
    let path = PathBuf::from("x.txt");
    assert_eq!(EntryError::EmptyName.to_string(), "the name is empty");
    assert_eq!(
        EntryError::AlreadyExists(path.clone()).to_string(),
        "x.txt already exists"
    );
    assert!(
        EntryError::NotAName("a/b".into())
            .to_string()
            .contains("a/b")
    );
    assert!(
        EntryError::IntoItself(path.clone())
            .to_string()
            .contains("x.txt")
    );
    let io = EntryError::Io {
        path,
        source: std::io::Error::other("denied"),
    };
    assert_eq!(io.to_string(), "cannot change x.txt: denied");
}
