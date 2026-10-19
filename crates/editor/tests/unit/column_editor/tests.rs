//! The Column Editor driven as a user drives it.

use gpui_kit::{TestAppContext, VisualTestContext};

use super::*;
use crate::test_support::{active_text, click, column_editor, has_dialog, open_workspace};

/// The dialog opened last.
fn opened() -> Entity<ColumnEditor> {
    OPENED
        .with(|opened| opened.borrow().clone().and_then(|weak| weak.upgrade()))
        .expect("the Column Editor is open")
}

fn fill(initial: &str, step: &str, repeat: &str, cx: &mut VisualTestContext) {
    let editor = opened();
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.fill_numbers(initial, step, repeat, window, cx);
        });
    });
}

fn shown_error(cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    cx.debug_bounds("column-error")?;
    opened().read_with(cx, |editor, _| editor.error.clone())
}

#[gpui_kit::test]
fn text_goes_in_at_the_caret_with_ok(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_keystrokes(column_editor());
    assert!(has_dialog(cx));
    // The text field has the focus.
    cx.simulate_input("ab");
    click("dialog-action", cx);
    assert!(!has_dialog(cx), "OK closes it");
    assert_eq!(active_text(&workspace, cx), "ab");
}

#[gpui_kit::test]
fn numbers_in_the_chosen_format_down_the_lines(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("x\nx\nx");
    cx.simulate_keystrokes(crate::test_support::document_start());
    cx.simulate_keystrokes(column_editor());
    click("column-mode-1", cx);
    click("column-format-1", cx);
    click("column-leading-1", cx);
    // Upper case is on for hexadecimal; off, the digits are small.
    click("column-uppercase", cx);
    let chosen = opened().read_with(cx, |editor, _| {
        (
            editor.numbers,
            editor.format,
            editor.leading,
            editor.uppercase,
        )
    });
    assert_eq!(chosen, (true, FormatName::Hex, LeadingName::Zeros, false));
    fill("e", "1", "", cx);
    click("dialog-action", cx);
    assert_eq!(active_text(&workspace, cx), "0ex\n0fx\n10x");
}

#[gpui_kit::test]
fn numbers_that_do_not_read_keep_the_dialog_open(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("x\nx");
    cx.simulate_keystrokes(crate::test_support::document_start());
    cx.simulate_keystrokes(column_editor());
    click("column-mode-1", cx);
    for (initial, step, repeat) in [
        ("z", "1", "1"),
        ("1", "q", "1"),
        ("1", "1", "0"),
        ("1", "1", "-2"),
    ] {
        fill(initial, step, repeat, cx);
        click("dialog-action", cx);
        assert!(has_dialog(cx), "{initial} {step} {repeat}");
        assert!(shown_error(cx).is_some(), "{initial} {step} {repeat}");
        assert_eq!(active_text(&workspace, cx), "x\nx");
    }
    // Another choice takes the message away.
    click("column-leading-2", cx);
    assert_eq!(shown_error(cx), None);
    // In binary, 2 is not a digit.
    click("column-format-3", cx);
    fill("2", "1", "1", cx);
    click("dialog-action", cx);
    assert!(shown_error(cx).is_some());
    // Empty step and repeat: the same number on every line, once each.
    click("column-format-0", cx);
    fill("7", "", "", cx);
    click("dialog-action", cx);
    assert!(!has_dialog(cx));
    assert_eq!(active_text(&workspace, cx), "7x\n7x");
}

#[gpui_kit::test]
fn cancel_changes_nothing(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_keystrokes(column_editor());
    cx.simulate_input("never");
    click("dialog-close", cx);
    assert!(!has_dialog(cx));
    assert_eq!(active_text(&workspace, cx), "");
}
