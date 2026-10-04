//! Tests of files changed by other programs: reloading with or without asking, unsaved
//! changes, deleted files, detection modes, the user's own saves, questions waiting for the
//! window, and monitoring (tail -f).

use std::fs;
use std::io::Write as _;
use std::path::Path;

use birchpad_commands::Invocation;
use birchpad_config::ChangeDetection;
use birchpad_core::{Range, Selection};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use crate::app_state::AppState;
use crate::buffer::ReadOnly;
use crate::editor::EditorView;
use crate::workspace::Workspace;
use crate::workspace::tests::{active_text, open_workspace, secondary, tab_names};

fn open(workspace: &Entity<Workspace>, path: &Path, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(path, window, cx)
    });
    cx.run_until_parked();
}

fn check(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.check_files(window, cx)
    });
    cx.run_until_parked();
}

fn answer(button: &str, cx: &mut VisualTestContext) {
    assert!(cx.has_pending_prompt(), "a question about {button}");
    cx.simulate_prompt_answer(button);
    cx.run_until_parked();
}

fn active(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<EditorView> {
    workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap())
}

/// The caret, whether the document is modified, and its bookmarks.
fn state(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> (Range, bool, Vec<usize>) {
    active(workspace, cx).read_with(cx, |view, cx| {
        let buffer = view.buffer.read(cx);
        (
            view.selection.primary(),
            buffer.is_modified(),
            buffer.bookmark_lines(),
        )
    })
}

fn settings(cx: &mut VisualTestContext, change: impl FnOnce(&mut birchpad_config::FileSettings)) {
    cx.update(|_, cx| change(&mut cx.global_mut::<AppState>().settings.files));
}

#[gpui_kit::test]
fn a_changed_file_reloads_on_the_same_lines(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, "one\ntwo\nthree\n").unwrap();
    let (workspace, cx) = open_workspace(cx);
    open(&workspace, &path, cx);
    // The caret after "tw", a bookmark on "two".
    active(&workspace, cx).update(cx, |view, cx| {
        view.selection = Selection::point(6);
        cx.notify();
    });
    cx.simulate_keystrokes("ctrl-f2");
    check(&workspace, cx);
    assert!(!cx.has_pending_prompt(), "unchanged");

    fs::write(&path, "zero\none\ntwo\nthree\n").unwrap();
    check(&workspace, cx);
    answer("Reload", cx);
    assert_eq!(active_text(&workspace, cx), "zero\none\ntwo\nthree\n");
    // Same line number and column: Notepad++ keeps the position too.
    assert_eq!(state(&workspace, cx), (Range::point(7), false, vec![1]));
    check(&workspace, cx);
    assert!(!cx.has_pending_prompt(), "nothing new");
}

#[gpui_kit::test]
fn silent_reloads_can_go_to_the_end(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, "one").unwrap();
    let (workspace, cx) = open_workspace(cx);
    settings(cx, |files| {
        files.reload_silently = true;
        files.reload_scrolls_to_end = true;
    });
    open(&workspace, &path, cx);
    fs::write(&path, "one\ntwo").unwrap();
    check(&workspace, cx);
    assert!(!cx.has_pending_prompt());
    assert_eq!(active_text(&workspace, cx), "one\ntwo");
    assert_eq!(state(&workspace, cx).0, Range::point(7), "at the end");
}

#[gpui_kit::test]
fn unsaved_changes_are_kept_or_lost_as_the_user_says(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, "disk").unwrap();
    let (workspace, cx) = open_workspace(cx);
    settings(cx, |files| files.reload_silently = true);
    open(&workspace, &path, cx);
    cx.simulate_input("mine ");

    // Even with silent reloads, unsaved changes are asked about.
    fs::write(&path, "disk 2").unwrap();
    check(&workspace, cx);
    answer("Keep My Changes", cx);
    assert_eq!(active_text(&workspace, cx), "mine disk");
    check(&workspace, cx);
    assert!(!cx.has_pending_prompt(), "that change was answered");

    fs::write(&path, "disk 3!").unwrap();
    check(&workspace, cx);
    answer("Reload", cx);
    assert_eq!(active_text(&workspace, cx), "disk 3!");
    assert!(!state(&workspace, cx).1);
}

#[gpui_kit::test]
fn deleted_files_are_kept_or_closed(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let [a, b] = ["a.txt", "b.txt"].map(|name| dir.path().join(name));
    fs::write(&a, "a").unwrap();
    fs::write(&b, "b").unwrap();
    let (workspace, cx) = open_workspace(cx);
    open(&workspace, &a, cx);
    open(&workspace, &b, cx);

    fs::remove_file(&b).unwrap();
    check(&workspace, cx);
    answer("Keep in Editor", cx);
    assert_eq!(active_text(&workspace, cx), "b");
    assert!(state(&workspace, cx).1, "unsaved: the file is gone");
    check(&workspace, cx);
    assert!(!cx.has_pending_prompt());

    fs::remove_file(&a).unwrap();
    check(&workspace, cx);
    answer("Close", cx);
    assert_eq!(tab_names(&workspace, cx), ["b.txt"]);
}

#[gpui_kit::test]
fn detection_can_watch_the_current_file_or_nothing(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let [a, b] = ["a.txt", "b.txt"].map(|name| dir.path().join(name));
    fs::write(&a, "a").unwrap();
    fs::write(&b, "b").unwrap();
    let (workspace, cx) = open_workspace(cx);
    open(&workspace, &a, cx);
    open(&workspace, &b, cx);
    fs::write(&a, "a 2").unwrap();
    fs::write(&b, "b 2").unwrap();

    settings(cx, |files| files.change_detection = ChangeDetection::Off);
    check(&workspace, cx);
    assert!(!cx.has_pending_prompt());

    // Only the active document, b.txt; a.txt when it becomes active.
    settings(cx, |files| {
        files.change_detection = ChangeDetection::Current
    });
    check(&workspace, cx);
    answer("Reload", cx);
    assert_eq!(active_text(&workspace, cx), "b 2");
    assert!(!cx.has_pending_prompt(), "a.txt is not asked about");
    cx.simulate_keystrokes("ctrl-tab");
    check(&workspace, cx);
    answer("Reload", cx);
    assert_eq!(active_text(&workspace, cx), "a 2");
}

#[gpui_kit::test]
fn the_users_own_saves_are_no_change(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, "a").unwrap();
    let (workspace, cx) = open_workspace(cx);
    open(&workspace, &path, cx);
    cx.simulate_input("saved ");
    cx.simulate_keystrokes(&secondary("s"));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&path).unwrap(), "saved a");
    check(&workspace, cx);
    assert!(!cx.has_pending_prompt());
}

#[gpui_kit::test]
fn questions_wait_for_the_window(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, "a").unwrap();
    let (workspace, cx) = open_workspace(cx);
    open(&workspace, &path, cx);
    cx.deactivate_window();
    fs::write(&path, "a 2").unwrap();
    check(&workspace, cx);
    assert!(
        !cx.has_pending_prompt(),
        "not while another program is in front"
    );

    // Back in front: the check runs and asks.
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    answer("Reload", cx);
    assert_eq!(active_text(&workspace, cx), "a 2");
}

#[gpui_kit::test]
fn monitoring_follows_what_is_appended(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("log.txt");
    fs::write(&path, "first\n").unwrap();
    let (workspace, cx) = open_workspace(cx);
    let run = |cx: &mut VisualTestContext| {
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.dispatch(&Invocation::new("view.monitoring"), window, cx)
        })
    };
    // Untitled documents have no file to follow.
    assert!(run(cx).is_err());
    open(&workspace, &path, cx);
    run(cx).unwrap();
    cx.run_until_parked();
    let read_only =
        active(&workspace, cx).read_with(cx, |view, cx| view.buffer.read(cx).read_only());
    assert_eq!(read_only, Some(ReadOnly::Monitoring));
    cx.simulate_input("typed");
    assert_eq!(active_text(&workspace, cx), "first\n", "read-only");

    let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
    file.write_all("second ж\n".as_bytes()).unwrap();
    drop(file);
    check(&workspace, cx);
    assert!(!cx.has_pending_prompt());
    assert_eq!(active_text(&workspace, cx), "first\nsecond ж\n");
    let (caret, modified, _) = state(&workspace, cx);
    assert_eq!(
        (caret, modified),
        (
            Range::point(
                "first
second ж
"
                .len()
            ),
            false
        ),
        "at the end"
    );

    // Rewritten from the start: read again.
    fs::write(&path, "new\n").unwrap();
    check(&workspace, cx);
    assert_eq!(active_text(&workspace, cx), "new\n");

    run(cx).unwrap();
    let read_only =
        active(&workspace, cx).read_with(cx, |view, cx| view.buffer.read(cx).read_only());
    assert_eq!(read_only, None);
}
