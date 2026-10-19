use std::fs;

use gpui_kit::{TestAppContext, VisualTestContext};

use super::*;
use crate::workspace::tests::{active_text, open_workspace, secondary, tab_names};

fn jump(a: &usize, b: &usize) -> bool {
    a.abs_diff(*b) >= JUMP_LINES
}

#[test]
fn small_moves_only_update_the_current_place() {
    let mut history = History::default();
    for line in [0, 5, 9, 14] {
        history.visit(line, jump);
    }
    assert!(!history.can_go(true));
    assert_eq!(history.back(), None);
    assert_eq!(history.forward(), None);
    assert!(!history.can_go(false));
}

#[test]
fn a_jump_pushes_the_place_the_caret_left() {
    let mut history = History::default();
    history.visit(0, jump);
    history.visit(3, jump);
    // Exactly ten lines away is a jump; nine is not.
    history.visit(13, jump);
    history.visit(21, jump);
    assert_eq!(
        history.back(),
        Some(3),
        "where the caret was, not where it started"
    );
    assert_eq!(history.back(), None);
    assert_eq!(history.forward(), Some(21));
    assert_eq!(history.forward(), None);
    assert_eq!(history.back(), Some(3));
}

#[test]
fn a_new_jump_drops_the_places_ahead() {
    let mut history = History::default();
    for line in [0, 100, 200] {
        history.visit(line, jump);
    }
    assert_eq!(history.back(), Some(100));
    assert!(history.can_go(false));
    history.visit(500, jump);
    assert!(!history.can_go(false), "the forward stack is gone");
    assert_eq!(history.back(), Some(100));
    assert_eq!(history.back(), Some(0));
}

#[test]
fn each_stack_keeps_the_last_fifty_places() {
    let mut history = History::default();
    for line in 0..=LIMIT + 10 {
        history.visit(line * 100, jump);
    }
    let mut back = Vec::new();
    while let Some(line) = history.back() {
        back.push(line);
    }
    assert_eq!(back.len(), LIMIT);
    assert_eq!(back.last(), Some(&1000), "the oldest ten are gone");
    let mut forward = 0;
    while history.forward().is_some() {
        forward += 1;
    }
    assert_eq!(forward, LIMIT);
}

// --- In the window -------------------------------------------------------------------------

fn back() -> &'static str {
    if cfg!(target_os = "macos") {
        "ctrl--"
    } else {
        "alt-left"
    }
}

fn forward() -> &'static str {
    if cfg!(target_os = "macos") {
        "ctrl-shift--"
    } else {
        "alt-right"
    }
}

/// The active document's name and caret.
fn place(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> (String, usize) {
    workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        let view = view.read(cx);
        (
            view.buffer.read(cx).display_name(),
            view.selection.primary().head,
        )
    })
}

fn go_to(workspace: &Entity<Workspace>, pos: usize, cx: &mut VisualTestContext) {
    workspace.update(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.update(cx, |view, cx| view.go_to(pos, cx));
    });
    cx.run_until_parked();
}

/// Forty numbered lines: line `n` (1-based) starts at `(n - 1) * 3`.
fn lines() -> String {
    (1..=40).map(|n| format!("{n:02}\n")).collect()
}

#[gpui_kit::test]
fn back_and_forward_within_a_document(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input(&lines());
    // From the end of the text, line 41, to line 2: a jump.
    go_to(&workspace, 3, cx);
    go_to(&workspace, 6, cx);
    // Line 31: a jump from line 3.
    go_to(&workspace, 90, cx);
    cx.simulate_keystrokes(back());
    assert_eq!(place(&workspace, cx), ("new 1".into(), 6));
    cx.simulate_keystrokes(forward());
    assert_eq!(place(&workspace, cx), ("new 1".into(), 90));
    // Nothing further either way.
    cx.simulate_keystrokes(forward());
    assert_eq!(place(&workspace, cx).1, 90);
    cx.simulate_keystrokes(back());
    cx.simulate_keystrokes(back());
    assert_eq!(place(&workspace, cx).1, 120, "the end of the typed text");
    cx.simulate_keystrokes(back());
    assert_eq!(place(&workspace, cx).1, 120, "typing the lines was no jump");
}

#[gpui_kit::test]
fn typing_and_pasting_lines_are_no_jumps(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    for _ in 0..15 {
        cx.simulate_input("line\n");
    }
    cx.simulate_input(&lines());
    cx.simulate_keystrokes(back());
    assert_eq!(place(&workspace, cx).1, 75 + 120, "still at the end");
}

#[gpui_kit::test]
fn back_returns_to_the_other_document_and_places_follow_edits(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("first");
    go_to(&workspace, 2, cx);
    cx.simulate_keystrokes(&secondary("n"));
    cx.simulate_input("second");
    cx.simulate_keystrokes(back());
    assert_eq!(place(&workspace, cx), ("new 1".into(), 2));
    cx.simulate_keystrokes(forward());
    assert_eq!(place(&workspace, cx), ("new 2".into(), 6));

    // Text typed before the place in new 1 moves it along.
    cx.simulate_keystrokes(back());
    go_to(&workspace, 0, cx);
    cx.simulate_input(">>");
    assert_eq!(active_text(&workspace, cx), ">>first");
    cx.simulate_keystrokes(forward());
    cx.simulate_keystrokes(back());
    assert_eq!(
        place(&workspace, cx),
        ("new 1".into(), 2),
        "the caret after >>"
    );
}

#[gpui_kit::test]
fn a_closed_file_opens_again_and_a_closed_untitled_document_is_skipped(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, lines()).unwrap();
    let (workspace, cx) = open_workspace(cx);
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(&path, window, cx)
    });
    cx.run_until_parked();
    go_to(&workspace, 60, cx);
    cx.simulate_keystrokes(&secondary("n"));
    cx.simulate_input("untitled");
    // Typed in, or opening a.txt again would replace it as an untouched document.
    cx.simulate_keystrokes(&secondary("n"));
    cx.simulate_input("x");
    assert_eq!(tab_names(&workspace, cx), ["a.txt", "new 1", "new 2"]);

    let close = |name: &str, cx: &mut VisualTestContext| {
        workspace.update_in(cx, |workspace, window, cx| {
            let view = workspace
                .all_views(cx)
                .into_iter()
                .find(|view| view.read(cx).buffer.read(cx).display_name() == name)
                .unwrap();
            workspace.close_view(&view, window, cx);
        });
        cx.run_until_parked();
    };
    close("a.txt", cx);
    close("new 1", cx);
    assert_eq!(tab_names(&workspace, cx), ["new 2"]);
    cx.simulate_keystrokes(back());
    cx.run_until_parked();
    assert_eq!(
        place(&workspace, cx),
        ("a.txt".into(), 60),
        "new 1 is gone for good; a.txt opens again at its place"
    );
    // And forward returns to new 2.
    cx.simulate_keystrokes(forward());
    assert_eq!(place(&workspace, cx).0, "new 2");
}

#[gpui_kit::test]
fn the_menu_commands_do_nothing_without_history(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    for command in ["navigate.back", "navigate.forward"] {
        workspace.update_in(cx, |workspace, window, cx| {
            workspace
                .dispatch(&birchpad_commands::Invocation::new(command), window, cx)
                .unwrap();
        });
    }
    assert_eq!(place(&workspace, cx), ("new 1".into(), 0));
}
