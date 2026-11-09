//! Tools > Git and the Git margin (ADR 0066), with the git program when this computer has it.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use birchpad::app_state::AppState;
use birchpad::editor::{EditorView, GitMark};
use birchpad::git::*;
use birchpad::workspace::Workspace;
use birchpad_commands::Invocation;
use birchpad_git::Git;
use birchpad_ui::take_warnings;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use crate::workspace::open_workspace;

/// Git, when this computer has it.
fn git() -> Option<Git> {
    let git = Git::default();
    git.run(Path::new("."), &["--version"], None).ok()?;
    Some(git)
}

/// A repository with `file.txt` committed as `text`.
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

fn wait_for(
    cx: &mut VisualTestContext,
    mut done: impl FnMut(&mut VisualTestContext) -> bool,
) -> bool {
    for _ in 0..200 {
        cx.run_until_parked();
        if done(cx) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
        cx.executor().advance_clock(Duration::from_millis(100));
    }
    cx.run_until_parked();
    done(cx)
}

fn run(
    workspace: &Entity<Workspace>,
    invocation: Invocation,
    cx: &mut VisualTestContext,
) -> Result<()> {
    let result = workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&invocation, window, cx)
    });
    cx.run_until_parked();
    result
}

fn open(
    workspace: &Entity<Workspace>,
    path: &Path,
    cx: &mut VisualTestContext,
) -> Entity<EditorView> {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(path, window, cx)
    });
    cx.run_until_parked();
    workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap())
}

fn marks_of(
    view: &Entity<EditorView>,
    cx: &mut VisualTestContext,
) -> Option<Vec<(std::ops::Range<usize>, GitMark)>> {
    let buffer = view.read_with(cx, |view, _| view.buffer.entity_id());
    cx.update(|_, cx| marks(buffer, cx))
        .map(|marks| marks.to_vec())
}

fn caret_line(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> usize {
    view.read_with(cx, |view, cx| {
        birchpad_core::motion::line_of(view.text(cx), view.selection.primary().head)
    })
}

fn setup<'a>(
    text: &str,
    cx: &'a mut TestAppContext,
) -> Option<(
    Entity<Workspace>,
    &'a mut VisualTestContext,
    tempfile::TempDir,
    PathBuf,
)> {
    let git = git()?;
    let dir = repository(&git, text);
    let (workspace, cx) = open_workspace(cx);
    cx.executor().allow_parking();
    let file = dir.path().join("file.txt");
    Some((workspace, cx, dir, file))
}

#[gpui_kit::test]
fn the_margin_marks_the_lines_changed_since_the_last_commit(cx: &mut TestAppContext) {
    let Some((workspace, cx, _dir, file)) = setup("one\ntwo\nthree\n", cx) else {
        return;
    };
    let view = open(&workspace, &file, cx);
    assert!(wait_for(cx, |cx| marks_of(&view, cx).is_some()), "tracked");
    assert_eq!(marks_of(&view, cx).unwrap(), []);
    // A line added after the first, the last modified.
    view.update(cx, |view, cx| view.go_to(4, cx));
    run(
        &workspace,
        Invocation::with_args("edit.insert-text", json!({ "text": "new\n" })),
        cx,
    )
    .unwrap();
    assert!(wait_for(cx, |cx| marks_of(&view, cx)
        .is_some_and(|marks| !marks.is_empty())));
    assert_eq!(marks_of(&view, cx).unwrap(), [(1..2, GitMark::Added)]);
    run(&workspace, Invocation::new("edit.select-all"), cx).unwrap();
    run(
        &workspace,
        Invocation::with_args("edit.insert-text", json!({ "text": "one\ntwo\n" })),
        cx,
    )
    .unwrap();
    assert!(wait_for(cx, |cx| marks_of(&view, cx).is_some_and(
        |marks| marks.iter().any(|(_, mark)| *mark == GitMark::Deleted)
    )));
    assert_eq!(marks_of(&view, cx).unwrap(), [(2..3, GitMark::Deleted)]);
}

#[gpui_kit::test]
fn next_and_previous_change_go_around(cx: &mut TestAppContext) {
    let Some((workspace, cx, _dir, file)) = setup("a\nb\nc\nd\ne\n", cx) else {
        return;
    };
    std::fs::write(&file, "a\nB\nc\nd\nE\n").unwrap();
    let view = open(&workspace, &file, cx);
    assert!(wait_for(cx, |cx| marks_of(&view, cx)
        .is_some_and(|marks| marks.len() == 2)));
    run(&workspace, Invocation::new("git.next-change"), cx).unwrap();
    assert_eq!(caret_line(&view, cx), 1);
    run(&workspace, Invocation::new("git.next-change"), cx).unwrap();
    assert_eq!(caret_line(&view, cx), 4);
    run(&workspace, Invocation::new("git.next-change"), cx).unwrap();
    assert_eq!(caret_line(&view, cx), 1, "around the end");
    run(&workspace, Invocation::new("git.previous-change"), cx).unwrap();
    assert_eq!(caret_line(&view, cx), 4, "around the start");
    // Committed outside Birchpad: back in front, the margin is read again, empty.
    let git = Git::default();
    git.run(
        file.parent().unwrap(),
        &["commit", "-q", "-a", "-m", "Second"],
        None,
    )
    .unwrap();
    workspace.update(cx, |workspace, cx| {
        refresh_heads(cx.entity_id(), cx);
        let _ = workspace;
    });
    assert!(wait_for(cx, |cx| marks_of(&view, cx)
        .is_some_and(|marks| marks.is_empty())));
    let error = run(&workspace, Invocation::new("git.next-change"), cx).unwrap_err();
    assert!(error.to_string().contains("no changes"), "{error}");
}

#[gpui_kit::test]
fn blame_shows_the_lines_commit(cx: &mut TestAppContext) {
    let Some((workspace, cx, _dir, file)) = setup("one\ntwo\n", cx) else {
        return;
    };
    let view = open(&workspace, &file, cx);
    let tip = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| view.call_tip.as_ref().map(|tip| tip.text()))
    };
    view.update(cx, |view, cx| view.go_to(5, cx));
    run(&workspace, Invocation::new("git.blame"), cx).unwrap();
    assert!(wait_for(cx, |cx| tip(cx).is_some()));
    let shown = tip(cx).unwrap();
    assert!(shown.contains("Ada Lovelace"), "{shown}");
    assert!(shown.contains("First version"), "{shown}");
    // A line typed, not committed.
    view.update(cx, |view, cx| {
        view.call_tip = None;
        view.go_to(0, cx);
    });
    run(
        &workspace,
        Invocation::with_args("edit.insert-text", json!({ "text": "typed\n" })),
        cx,
    )
    .unwrap();
    view.update(cx, |view, cx| view.go_to(0, cx));
    run(&workspace, Invocation::new("git.blame"), cx).unwrap();
    assert!(wait_for(cx, |cx| tip(cx).is_some()));
    assert_eq!(tip(cx).as_deref(), Some("Not committed yet"));
}

#[gpui_kit::test]
fn documents_git_does_not_track_have_no_margin(cx: &mut TestAppContext) {
    let Some((workspace, cx, dir, _)) = setup("x\n", cx) else {
        return;
    };
    // Untracked in the repository, and outside one.
    let untracked = dir.path().join("new.txt");
    std::fs::write(&untracked, "new\n").unwrap();
    let view = open(&workspace, &untracked, cx);
    wait_for(cx, |_| false);
    assert_eq!(marks_of(&view, cx), None);
    let error = run(&workspace, Invocation::new("git.next-change"), cx).unwrap_err();
    assert!(error.to_string().contains("does not track"), "{error}");
    let outside = tempfile::tempdir().unwrap();
    let alone = outside.path().join("alone.txt");
    std::fs::write(&alone, "x").unwrap();
    let view = open(&workspace, &alone, cx);
    wait_for(cx, |_| false);
    assert_eq!(marks_of(&view, cx), None);
    // Untitled: saved first.
    run(&workspace, Invocation::new("file.new"), cx).unwrap();
    for command in ["git.blame", "git.previous-change"] {
        let error = run(&workspace, Invocation::new(command), cx).unwrap_err();
        assert!(
            error.to_string().contains("save the document first"),
            "{command}: {error}"
        );
    }
}

#[gpui_kit::test]
fn git_turned_off_is_not_asked(cx: &mut TestAppContext) {
    let Some((workspace, cx, _dir, file)) = setup("x\n", cx) else {
        return;
    };
    cx.update(|_, cx| cx.global_mut::<AppState>().settings.git.enabled = false);
    let view = open(&workspace, &file, cx);
    wait_for(cx, |_| false);
    assert_eq!(marks_of(&view, cx), None);
    for command in ["git.blame", "git.next-change", "compare.with-git"] {
        let error = run(&workspace, Invocation::new(command), cx).unwrap_err();
        assert!(
            error.to_string().contains("Git is turned off"),
            "{command}: {error}"
        );
    }
    // Turned on again: the margin comes.
    cx.update(|_, cx| cx.global_mut::<AppState>().settings.git.enabled = true);
    workspace.update(cx, |workspace, cx| workspace.refresh_views(cx));
    assert!(wait_for(cx, |cx| marks_of(&view, cx).is_some()));
}

#[gpui_kit::test]
fn a_git_program_that_does_not_run_is_told_once(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.executor().allow_parking();
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().settings.git.program = "birchpad-no-such-git".into();
    });
    take_warnings();
    let dir = tempfile::tempdir().unwrap();
    for name in ["a.txt", "b.txt"] {
        let path = dir.path().join(name);
        std::fs::write(&path, "x").unwrap();
        let view = open(&workspace, &path, cx);
        wait_for(cx, |_| false);
        assert_eq!(marks_of(&view, cx), None);
    }
    let warnings = take_warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("birchpad-no-such-git"), "{warnings:?}");
    // Blame says it too.
    birchpad_ui::take_errors();
    run(&workspace, Invocation::new("git.blame"), cx).unwrap();
    assert!(wait_for(cx, |_| !birchpad_ui::take_errors().is_empty()));
}
