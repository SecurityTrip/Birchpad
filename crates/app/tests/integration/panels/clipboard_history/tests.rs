use gpui_kit::{ClipboardItem, Entity, Modifiers, TestAppContext, VisualTestContext};

use crate::workspace::{active_text, open_workspace};
use birchpad::panels::PanelKind;
use birchpad::panels::clipboard_history::*;
use birchpad::workspace::Workspace;
use std::collections::VecDeque;

#[test]
fn the_newest_text_comes_first_once() {
    let mut history = VecDeque::new();
    assert!(remember(&mut history, "a"));
    assert!(remember(&mut history, "b"));
    assert!(!remember(&mut history, "b"), "the same as the top");
    assert!(remember(&mut history, "a"), "moves up");
    assert_eq!(history, ["a", "b"]);
    // Empty and huge texts are not kept.
    assert!(!remember(&mut history, ""));
    assert!(!remember(&mut history, &"x".repeat(MAX_TEXT + 1)));
    assert!(
        remember(&mut history, &"x".repeat(MAX_TEXT)),
        "exactly the limit"
    );
    // The oldest go past the limit.
    for n in 0..LIMIT + 5 {
        remember(&mut history, &n.to_string());
    }
    assert_eq!(history.len(), LIMIT);
    assert_eq!(history.front().map(String::as_str), Some("34"));
    assert_eq!(history.back().map(String::as_str), Some("5"));
}

#[test]
fn previews_are_one_line_and_short() {
    assert_eq!(preview("one\r\ntwo\nthree\tfour"), "one⏎two⏎three four");
    assert_eq!(preview(""), "");
    let long = "ж".repeat(PREVIEW);
    assert_eq!(preview(&long), long, "exactly the length shown");
    let longer = "ж".repeat(PREVIEW + 1);
    assert_eq!(preview(&longer), format!("{long}…"));
}

fn panel(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<ClipboardHistory> {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_panel(PanelKind::ClipboardHistory, window, cx);
        let view = workspace.docks.view(PanelKind::ClipboardHistory).unwrap();
        view.clone().downcast::<ClipboardHistory>().unwrap()
    })
}

#[gpui_kit::test]
fn copies_are_collected_and_pasted_back(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.write_to_clipboard(ClipboardItem::new_string("first".into()));
    let history = panel(&workspace, cx);
    cx.run_until_parked();
    let entries = |cx: &mut VisualTestContext| {
        history.read_with(cx, |history, _| {
            history.entries.iter().cloned().collect::<Vec<_>>()
        })
    };
    assert_eq!(entries(cx), ["first"], "read when the panel opens");
    // A copy in the editor shows within the next second.
    cx.simulate_input("typed text");
    cx.simulate_keystrokes(&crate::workspace::secondary("a"));
    cx.simulate_keystrokes(&crate::workspace::secondary("c"));
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    assert_eq!(entries(cx), ["typed text", "first"]);

    // A double-click pastes the text at the caret.
    cx.simulate_keystrokes("end");
    let row = cx.debug_bounds("clipboard-row-1").expect("drawn").center();
    cx.simulate_event(gpui_kit::MouseDownEvent {
        position: row,
        button: gpui_kit::MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(gpui_kit::MouseUpEvent {
        position: row,
        button: gpui_kit::MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 2,
    });
    cx.run_until_parked();
    assert_eq!(active_text(&workspace, cx), "typed textfirst");

    // Keys: Delete forgets the selected text, Enter pastes; nothing selected, nothing.
    workspace.update_in(cx, |_, window, cx| {
        let focus = history.read(cx).focus_handle.clone();
        window.focus(&focus, cx);
    });
    cx.simulate_keystrokes("up delete");
    assert_eq!(entries(cx), ["first"]);
    cx.simulate_keystrokes("enter");
    assert_eq!(active_text(&workspace, cx), "typed textfirstfirst");
    // Pasting goes back to the text; in the panel again, the history empties.
    workspace.update_in(cx, |_, window, cx| {
        let focus = history.read(cx).focus_handle.clone();
        window.focus(&focus, cx);
    });
    cx.simulate_keystrokes("delete delete enter");
    assert!(entries(cx).is_empty());
    assert_eq!(active_text(&workspace, cx), "typed textfirstfirst");
}
