#![allow(
    clippy::single_range_in_vec_init,
    reason = "expected highlights written out"
)]

use birchpad_core::{Range, Selection};
use gpui_kit::{TestAppContext, VisualTestContext};

use super::*;
use crate::workspace::tests::{document_start, open_workspace, secondary};

fn type_text(workspace: &Entity<Workspace>, text: &str, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace
            .incremental
            .update(cx, |bar, cx| bar.type_text(text, window, cx));
    });
    cx.run_until_parked();
}

fn selection(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> (usize, usize) {
    workspace.read_with(cx, |workspace, cx| {
        let range = workspace
            .active_view(cx)
            .unwrap()
            .read(cx)
            .selection
            .primary();
        (range.anchor, range.head)
    })
}

fn state(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> (Option<&'static str>, bool, Option<usize>) {
    workspace.read_with(cx, |workspace, cx| workspace.incremental.read(cx).state())
}

fn highlights(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> Vec<std::ops::Range<usize>> {
    workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        let buffer = view.read(cx).buffer.read(cx);
        buffer
            .marks()
            .incremental
            .iter()
            .map(|(range, _)| range)
            .collect()
    })
}

fn incremental() -> String {
    secondary("alt-i")
}

#[gpui_kit::test]
fn the_match_grows_with_the_text_and_enter_steps_through(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("cat car cart\ncar");
    cx.simulate_keystrokes(document_start());
    cx.simulate_keystrokes(&incremental());
    type_text(&workspace, "c", cx);
    assert_eq!(selection(&workspace, cx), (0, 1));
    type_text(&workspace, "car", cx);
    assert_eq!(
        selection(&workspace, cx),
        (4, 7),
        "from the start of the match"
    );
    type_text(&workspace, "cart", cx);
    assert_eq!(selection(&workspace, cx), (8, 12));
    assert_eq!(state(&workspace, cx), (None, false, None));

    // Enter: the next one, wrapping around; Shift+Enter back.
    type_text(&workspace, "car", cx);
    assert_eq!(
        selection(&workspace, cx),
        (8, 11),
        "cart still starts with car"
    );
    cx.simulate_keystrokes("enter");
    assert_eq!(selection(&workspace, cx), (13, 16));
    cx.simulate_keystrokes("enter");
    assert_eq!(selection(&workspace, cx), (4, 7));
    assert_eq!(state(&workspace, cx).0, Some("incremental.wrapped-to-top"));
    cx.simulate_keystrokes("shift-enter");
    assert_eq!(selection(&workspace, cx), (13, 16));
    assert_eq!(
        state(&workspace, cx).0,
        Some("incremental.wrapped-to-bottom")
    );
}

#[gpui_kit::test]
fn nothing_found_turns_red_and_an_empty_field_finds_nothing(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("alpha beta");
    cx.simulate_keystrokes(document_start());
    cx.simulate_keystrokes(&incremental());
    type_text(&workspace, "bet", cx);
    assert_eq!(selection(&workspace, cx), (6, 9));
    type_text(&workspace, "betx", cx);
    assert_eq!(
        state(&workspace, cx),
        (Some("incremental.not-found"), true, None)
    );
    assert_eq!(selection(&workspace, cx), (6, 9), "the last match stays");
    // Emptied: no message, and the caret where the match was.
    type_text(&workspace, "", cx);
    assert_eq!(state(&workspace, cx), (None, false, None));
    assert_eq!(selection(&workspace, cx), (6, 6));
    // Enter on an empty field does nothing.
    cx.simulate_keystrokes("enter shift-enter");
    assert_eq!(selection(&workspace, cx), (6, 6));
}

#[gpui_kit::test]
fn options_and_highlight_all(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("Word word sword word");
    cx.simulate_keystrokes(document_start());
    cx.simulate_keystrokes(&incremental());
    let set = |change: fn(&mut IncrementalBar), cx: &mut VisualTestContext| {
        workspace.update(cx, |workspace, cx| {
            workspace.incremental.update(cx, |bar, cx| {
                change(bar);
                cx.emit(IncrementalEvent::Changed);
            });
        });
        cx.run_until_parked();
    };
    set(|bar| bar.highlight_all = true, cx);
    type_text(&workspace, "word", cx);
    assert_eq!(state(&workspace, cx), (None, false, Some(4)));
    assert_eq!(highlights(&workspace, cx), [0..4, 5..9, 11..15, 16..20]);
    set(|bar| bar.match_case = true, cx);
    assert_eq!(state(&workspace, cx).2, Some(3));
    assert_eq!(selection(&workspace, cx), (5, 9), "Word no longer matches");
    set(|bar| bar.whole_word = true, cx);
    assert_eq!(highlights(&workspace, cx), [5..9, 16..20], "not in sword");
    // Off: the highlights go, and so does the count.
    set(|bar| bar.highlight_all = false, cx);
    assert!(highlights(&workspace, cx).is_empty());
    assert_eq!(state(&workspace, cx).2, None);
    // Nothing found with Highlight all: zero, and nothing highlighted.
    set(|bar| bar.highlight_all = true, cx);
    type_text(&workspace, "none", cx);
    assert_eq!(
        state(&workspace, cx),
        (Some("incremental.not-found"), true, Some(0))
    );
    assert!(highlights(&workspace, cx).is_empty());
}

#[gpui_kit::test]
fn escape_closes_and_the_selection_becomes_the_text(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("one two one");
    workspace.update(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.update(cx, |view, cx| {
            view.selection = Selection::single(Range::new(4, 7));
            cx.notify();
        });
        workspace
            .incremental
            .update(cx, |bar, _| bar.highlight_all = true);
    });
    cx.simulate_keystrokes(&incremental());
    let (visible, text) = workspace.read_with(cx, |workspace, cx| {
        let bar = workspace.incremental.read(cx);
        (bar.visible, bar.query(cx).pattern)
    });
    assert!(visible);
    assert_eq!(text, "two");
    assert_eq!(
        selection(&workspace, cx),
        (4, 7),
        "the selection is its own match"
    );
    assert_eq!(highlights(&workspace, cx), [4..7]);

    cx.simulate_keystrokes("escape");
    let (visible, focused) = workspace.update_in(cx, |workspace, window, cx| {
        let view = workspace.active_view(cx).unwrap();
        let focused = view.read(cx).focus_handle.is_focused(window);
        (workspace.incremental.read(cx).visible, focused)
    });
    assert!(!visible);
    assert!(focused, "back in the text");
    assert!(
        highlights(&workspace, cx).is_empty(),
        "highlights go with the bar"
    );
    // A selection over two lines is not taken as the text; nor is an empty one.
    workspace.update(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.update(cx, |view, _| view.selection = Selection::point(0));
    });
    cx.simulate_keystrokes(&incremental());
    let text = workspace.read_with(cx, |workspace, cx| {
        workspace.incremental.read(cx).query(cx).pattern
    });
    assert_eq!(text, "two", "the field keeps the last text");
    // Closing again when closed does nothing.
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.close_incremental(window, cx);
        workspace.close_incremental(window, cx);
    });
}

#[gpui_kit::test]
fn an_empty_document_and_the_same_text_again(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_keystrokes(&incremental());
    type_text(&workspace, "x", cx);
    assert_eq!(
        state(&workspace, cx),
        (Some("incremental.not-found"), true, None)
    );
    assert_eq!(selection(&workspace, cx), (0, 0));
    // Enter reports a change of the field without one: no search from the match's start.
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.incremental.update(cx, |bar, cx| {
            bar.status = Some("kept");
            bar.type_text("x", window, cx);
        });
    });
    assert_eq!(state(&workspace, cx).0, Some("kept"), "not searched again");
}
