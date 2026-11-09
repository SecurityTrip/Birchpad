use std::path::Path;

use birchpad_lsp::languages::*;

#[test]
fn protocol_language_ids() {
    assert_eq!(protocol_language_id("rust"), "rust");
    assert_eq!(protocol_language_id("python"), "python");
    assert_eq!(protocol_language_id("bash"), "shellscript");
    assert_eq!(protocol_language_id("batch"), "bat");
    assert_eq!(protocol_language_id("cs"), "csharp");
    assert_eq!(protocol_language_id("objc"), "objective-c");
    assert_eq!(protocol_language_id("tsx"), "typescriptreact");
    assert_eq!(protocol_language_id(""), "");
}

#[test]
fn the_root_is_the_nearest_folder_with_git() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let nested = project.join("src").join("deep");
    std::fs::create_dir_all(&nested).unwrap();
    let file = nested.join("main.rs");
    // No .git anywhere under the temporary folder: the file's own folder.
    assert_eq!(root_folder(&file), Some(nested.clone()));
    std::fs::create_dir(project.join(".git")).unwrap();
    assert_eq!(root_folder(&file), Some(project.clone()));
    // A worktree's .git is a file; the nearest wins.
    std::fs::write(project.join("src").join(".git"), "gitdir: elsewhere").unwrap();
    assert_eq!(root_folder(&file), Some(project.join("src")));
    // A file directly in the folder with .git.
    assert_eq!(
        root_folder(&project.join("Cargo.toml")),
        Some(project.clone())
    );
}

#[test]
fn a_path_without_a_folder_has_no_root() {
    assert_eq!(root_folder(Path::new("main.rs")), None);
    assert_eq!(root_folder(Path::new("")), None);
    let root = if cfg!(windows) { r"C:\" } else { "/" };
    assert_eq!(root_folder(Path::new(root)), None);
}
