use std::path::Path;

use birchpad_git::*;

/// Git, when this computer has it (CI does).
fn git() -> Option<Git> {
    let git = Git::default();
    git.run(Path::new("."), &["--version"], None).ok()?;
    Some(git)
}

/// A repository with `file.txt` committed.
fn repository(git: &Git, text: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| git.run(dir.path(), args, None).unwrap();
    run(&["init", "-q"]);
    run(&["config", "user.name", "Ada Lovelace"]);
    run(&["config", "user.email", "ada@example.com"]);
    run(&["config", "core.autocrlf", "false"]);
    std::fs::write(dir.path().join("file.txt"), text).unwrap();
    run(&["add", "file.txt"]);
    run(&["commit", "-q", "-m", "First version"]);
    dir
}

#[test]
fn the_program_is_git_unless_named() {
    assert_eq!(Git::default().program, "git");
    assert_eq!(Git::new("  ").program, "git");
    assert_eq!(Git::new(" /usr/bin/git ").program, "/usr/bin/git");
}

#[test]
fn a_program_that_does_not_run_says_so() {
    let git = Git::new("birchpad-no-such-git");
    let error = git.run(Path::new("."), &["status"], None).unwrap_err();
    assert!(matches!(error, GitError::CannotRun { .. }), "{error:?}");
    assert!(
        error.to_string().contains("birchpad-no-such-git"),
        "{error}"
    );
    // As for a file's HEAD text: an error, not "no text".
    let dir = tempfile::tempdir().unwrap();
    assert!(git.head_text(&dir.path().join("a.txt")).is_err());
}

#[test]
fn paths_without_a_folder_or_name_are_refused() {
    let git = Git::default();
    for path in ["", "file.txt"] {
        assert!(
            matches!(git.head_text(Path::new(path)), Err(GitError::NotAFile(_))),
            "{path:?}"
        );
        assert!(matches!(
            git.blame_line(Path::new(path), 0, b""),
            Err(GitError::NotAFile(_))
        ));
    }
}

#[test]
fn a_files_text_in_head() {
    let Some(git) = git() else { return };
    let dir = repository(&git, "one\ntwo\n");
    let file = dir.path().join("file.txt");
    std::fs::write(&file, "changed\n").unwrap();
    assert_eq!(
        git.head_text(&file).unwrap().as_deref(),
        Some(&b"one\ntwo\n"[..])
    );
    // Untracked, and outside a repository: none.
    std::fs::write(dir.path().join("new.txt"), "x").unwrap();
    assert_eq!(git.head_text(&dir.path().join("new.txt")).unwrap(), None);
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("a.txt"), "x").unwrap();
    assert_eq!(git.head_text(&outside.path().join("a.txt")).unwrap(), None);
    // A repository without a commit: none.
    git.run(outside.path(), &["init", "-q"], None).unwrap();
    assert_eq!(git.head_text(&outside.path().join("a.txt")).unwrap(), None);
    // A failure says what Git said.
    let error = git
        .run(dir.path(), &["show", "HEAD:./missing"], None)
        .unwrap_err();
    assert!(
        matches!(&error, GitError::Failed(said) if !said.is_empty()),
        "{error:?}"
    );
}

#[test]
fn a_lines_blame_with_the_documents_text() {
    let Some(git) = git() else { return };
    let dir = repository(&git, "one\ntwo\n");
    let file = dir.path().join("file.txt");
    let blame = git.blame_line(&file, 1, b"one\ntwo\n").unwrap();
    assert_eq!(blame.author, "Ada Lovelace");
    assert_eq!(blame.summary, "First version");
    assert!(!blame.is_uncommitted());
    // A line typed but not saved nor committed.
    let blame = git.blame_line(&file, 1, b"one\nnew\ntwo\n").unwrap();
    assert!(blame.is_uncommitted());
    // The first line, and a line past the end of the text.
    assert!(
        !git.blame_line(&file, 0, b"one\ntwo\n")
            .unwrap()
            .is_uncommitted()
    );
    assert!(matches!(
        git.blame_line(&file, 99, b"one\ntwo\n"),
        Err(GitError::Failed(_))
    ));
    // A file Git does not track.
    std::fs::write(dir.path().join("new.txt"), "x").unwrap();
    assert!(
        git.blame_line(&dir.path().join("new.txt"), 0, b"x")
            .is_err()
    );
}
