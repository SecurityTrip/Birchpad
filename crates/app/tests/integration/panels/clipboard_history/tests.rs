use gpui_kit::{ClipboardItem, Entity, Modifiers, TestAppContext, VisualTestContext};

use crate::workspace::{active_text, open_workspace};
use birchpad::panels::PanelKind;
use birchpad::panels::clipboard_history::*;
use birchpad::workspace::Workspace;

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
