use birchpad_commands::Invocation;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use crate::workspace::{active_text, open_workspace};
use anyhow::Result;
use birchpad::hex_editor::HexEditor;
use birchpad::workspace::Workspace;

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

fn type_text(workspace: &Entity<Workspace>, text: &str, cx: &mut VisualTestContext) {
    run(
        workspace,
        Invocation::with_args("edit.insert-text", json!({ "text": text })),
        cx,
    )
    .unwrap();
}

/// Opens View in HEX on the active document.
fn hex(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<HexEditor> {
    run(workspace, Invocation::new("hex.editor"), cx).unwrap();
    workspace.read_with(cx, |workspace, cx| workspace.active_hex_editor(cx).unwrap())
}

fn caret(editor: &Entity<HexEditor>, cx: &mut VisualTestContext) -> (usize, usize) {
    editor.read_with(cx, |editor, _| (editor.anchor, editor.cursor))
}

fn text_caret(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> (usize, usize) {
    workspace.read_with(cx, |workspace, cx| {
        let selection = workspace
            .active_view(cx)
            .unwrap()
            .read(cx)
            .selection
            .primary();
        (selection.anchor, selection.head)
    })
}

fn digits(editor: &Entity<HexEditor>, typed: &str, cx: &mut VisualTestContext) -> Result<()> {
    editor.update(cx, |editor, cx| {
        for digit in typed.chars().filter(|c| !c.is_whitespace()) {
            editor.type_digit(digit.to_digit(16).unwrap() as u8, cx)?;
        }
        Ok(())
    })
}

#[gpui_kit::test]
fn shows_the_bytes_in_place_and_shares_the_caret(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "Hi мир", cx);
    // The caret before "мир".
    workspace.update(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.update(cx, |view, cx| view.select_range(3..3, cx));
    });
    let editor = hex(&workspace, cx);
    let bytes = editor.update(cx, |editor, cx| editor.bytes(cx).to_vec());
    assert_eq!(bytes, "Hi мир".as_bytes());
    assert_eq!(caret(&editor, cx), (3, 3));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("hex-byte-0").is_some());
    assert!(cx.debug_bounds("hex-text-8").is_some());
    // The menu item shows it is on.
    let checked = workspace.update(cx, |workspace, cx| {
        workspace.refresh_menus(cx);
        workspace.active_hex_editor(cx).is_some()
    });
    assert!(checked);

    // Moving in the bytes moves the text's caret, to the start of the character.
    editor.update(cx, |editor, cx| editor.move_to(6, false, cx));
    assert_eq!(text_caret(&workspace, cx), (5, 5));
    editor.update(cx, |editor, cx| editor.move_to(1, true, cx));
    assert_eq!(text_caret(&workspace, cx), (5, 1));
    // Past the end: the end.
    editor.update(cx, |editor, cx| editor.move_to(999, false, cx));
    assert_eq!(caret(&editor, cx), (9, 9));
    // View in HEX again: the text, with the caret where the bytes' was.
    run(&workspace, Invocation::new("hex.editor"), cx).unwrap();
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.active_hex_editor(cx).is_none()
    }));
    assert_eq!(text_caret(&workspace, cx), (9, 9));
}

#[gpui_kit::test]
fn types_bytes_over_or_before_the_caret(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "abc", cx);
    let editor = hex(&workspace, cx);
    editor.update(cx, |editor, cx| editor.move_to(0, false, cx));
    // Overwrite: two digits make a byte; the first alone changes nothing yet.
    digits(&editor, "4", cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "abc");
    assert_eq!(editor.read_with(cx, |editor, _| editor.nibble), Some(4));
    digits(&editor, "2", cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "Bbc");
    assert_eq!(caret(&editor, cx), (1, 1));
    // Insert: before the caret.
    editor.update(cx, |editor, _| editor.overwrite = false);
    digits(&editor, "5A", cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "BZbc");
    // Past the end, overwriting adds.
    editor.update(cx, |editor, cx| {
        editor.overwrite = true;
        editor.move_to(4, false, cx);
    });
    digits(&editor, "21", cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "BZbc!");
    // Each byte typed is one undo step.
    run(&workspace, Invocation::new("edit.undo"), cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "BZbc");
}

#[gpui_kit::test]
fn holds_back_bytes_until_they_are_text(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "ab", cx);
    let editor = hex(&workspace, cx);
    editor.update(cx, |editor, cx| {
        editor.overwrite = false;
        editor.move_to(1, false, cx);
    });
    // é is C3 A9 in UTF-8: C3 alone is no text, so it waits.
    digits(&editor, "C3", cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "ab");
    assert_eq!(
        editor.read_with(cx, |editor, _| editor.pending.clone()),
        [0xC3]
    );
    digits(&editor, "A9", cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "aéb");
    assert!(editor.read_with(cx, |editor, _| editor.pending.is_empty()));
    // Bytes that never become text: refused after four, and nothing changed.
    let error = digits(&editor, "FF FF FF FF", cx).unwrap_err();
    assert!(error.to_string().contains("text (at byte"), "{error}");
    assert_eq!(active_text(&workspace, cx), "aéb");
    // Escape (or Backspace) drops what waits.
    digits(&editor, "E2 8", cx).unwrap();
    editor
        .update(cx, |editor, cx| editor.delete(false, cx))
        .unwrap();
    assert!(editor.read_with(cx, |editor, _| editor.pending.is_empty()
        && editor.nibble.is_none()));
    assert_eq!(active_text(&workspace, cx), "aéb");
}

#[gpui_kit::test]
fn the_text_column_types_characters(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "xyz", cx);
    let editor = hex(&workspace, cx);
    editor.update(cx, |editor, cx| {
        editor.in_text = true;
        editor.move_to(1, false, cx);
    });
    editor
        .update(cx, |editor, cx| editor.type_text("ж", cx))
        .unwrap();
    // Two bytes over two bytes.
    assert_eq!(active_text(&workspace, cx), "xж");
    assert_eq!(caret(&editor, cx), (3, 3));
}

#[gpui_kit::test]
fn deletes_whole_characters_and_selections(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "aжb", cx);
    let editor = hex(&workspace, cx);
    editor.update(cx, |editor, cx| {
        editor.overwrite = false;
        // Inside "ж": its second byte.
        editor.move_to(2, false, cx);
    });
    editor
        .update(cx, |editor, cx| editor.delete(true, cx))
        .unwrap();
    assert_eq!(active_text(&workspace, cx), "ab");
    editor.update(cx, |editor, cx| editor.move_to(2, false, cx));
    editor
        .update(cx, |editor, cx| editor.delete(false, cx))
        .unwrap();
    assert_eq!(active_text(&workspace, cx), "a");
    // At either end, nothing to delete.
    editor.update(cx, |editor, cx| editor.move_to(1, false, cx));
    editor
        .update(cx, |editor, cx| editor.delete(true, cx))
        .unwrap();
    editor.update(cx, |editor, cx| editor.move_to(0, false, cx));
    editor
        .update(cx, |editor, cx| editor.delete(false, cx))
        .unwrap();
    assert_eq!(active_text(&workspace, cx), "a");
    // A selection goes whole; typing replaces it.
    run(&workspace, Invocation::new("hex.editor"), cx).unwrap();
    run(&workspace, Invocation::new("edit.select-all"), cx).unwrap();
    type_text(&workspace, "abcd", cx);
    let editor = hex(&workspace, cx);
    editor.update(cx, |editor, cx| {
        editor.move_to(1, false, cx);
        editor.move_to(3, true, cx);
    });
    digits(&editor, "2D", cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "a-d");
    // Overwrite: Backspace only moves back.
    editor.update(cx, |editor, cx| {
        editor.overwrite = true;
        editor.move_to(2, false, cx);
    });
    editor
        .update(cx, |editor, cx| editor.delete(false, cx))
        .unwrap();
    assert_eq!(active_text(&workspace, cx), "a-d");
    assert_eq!(caret(&editor, cx), (1, 1));
}

#[gpui_kit::test]
fn finds_bytes_and_text(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "one two one", cx);
    let editor = hex(&workspace, cx);
    editor.update(cx, |editor, cx| editor.move_to(0, false, cx));
    let find = |pattern: &str, backward: bool, cx: &mut VisualTestContext| {
        run(
            &workspace,
            Invocation::with_args(
                "hex.find",
                json!({ "pattern": pattern, "backward": backward }),
            ),
            cx,
        )
    };
    find("6F 6E", false, cx).unwrap();
    assert_eq!(caret(&editor, cx), (0, 2));
    // Again: the next one, then round to the first.
    find("6f6e", false, cx).unwrap();
    assert_eq!(caret(&editor, cx), (8, 10));
    find("\"one\"", false, cx).unwrap();
    assert_eq!(caret(&editor, cx), (0, 3));
    find("\"one\"", true, cx).unwrap();
    assert_eq!(caret(&editor, cx), (8, 11));
    assert_eq!(text_caret(&workspace, cx), (8, 11));
    // Not there, or not a pattern.
    let error = find("FF", false, cx).unwrap_err();
    assert!(error.to_string().contains("not in the bytes"), "{error}");
    assert!(find("6", false, cx).is_err());
    assert!(find("zz", false, cx).is_err());
    assert!(find("", false, cx).is_err());
    // Without a pattern: the field.
    run(&workspace, Invocation::new("hex.find"), cx).unwrap();
    // Not in HEX: said so.
    run(&workspace, Invocation::new("hex.editor"), cx).unwrap();
    let error = find("6F", false, cx).unwrap_err();
    assert!(error.to_string().contains("not shown in HEX"), "{error}");
}

#[gpui_kit::test]
fn other_encodings_and_the_byte_order_mark(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "Aж", cx);
    run(
        &workspace,
        Invocation::with_args("encoding.convert-to", json!({ "encoding": "utf-16le-bom" })),
        cx,
    )
    .unwrap();
    workspace.update(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.update(cx, |view, cx| view.select_range(1..1, cx));
    });
    let editor = hex(&workspace, cx);
    let bytes = editor.update(cx, |editor, cx| editor.bytes(cx).to_vec());
    assert_eq!(bytes, [0xFF, 0xFE, 0x41, 0x00, 0x36, 0x04]);
    // The text's caret after "A" is byte 4, past the mark.
    assert_eq!(caret(&editor, cx), (4, 4));
    editor.update(cx, |editor, cx| editor.move_to(5, false, cx));
    assert_eq!(text_caret(&workspace, cx), (1, 1));
    // The mark is no text to edit.
    editor.update(cx, |editor, cx| editor.move_to(0, false, cx));
    let error = digits(&editor, "00", cx).unwrap_err();
    assert!(error.to_string().contains("byte order mark"), "{error}");
    // "A" becomes "B": 42 00.
    editor.update(cx, |editor, cx| editor.move_to(2, false, cx));
    digits(&editor, "42", cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "Bж");
}

#[gpui_kit::test]
fn a_read_only_document_is_not_changed(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "abc", cx);
    run(&workspace, Invocation::new("edit.toggle-read-only"), cx).unwrap();
    let editor = hex(&workspace, cx);
    editor.update(cx, |editor, cx| editor.move_to(0, false, cx));
    let error = digits(&editor, "41", cx).unwrap_err();
    assert!(error.to_string().contains("cannot be changed"), "{error}");
    assert_eq!(active_text(&workspace, cx), "abc");
}

#[gpui_kit::test]
fn rows_follow_the_preferences(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, &"x".repeat(20), cx);
    cx.update(|_, cx| {
        let hex = &mut cx
            .global_mut::<birchpad::app_state::AppState>()
            .settings
            .hex;
        hex.columns = 8;
        hex.group = 4;
        hex.uppercase = false;
    });
    let _editor = hex(&workspace, cx);
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    // Byte 8 starts the second row, below byte 0.
    let first = cx.debug_bounds("hex-byte-0").unwrap();
    let ninth = cx.debug_bounds("hex-byte-8").unwrap();
    assert!(ninth.origin.y > first.origin.y);
    assert_eq!(ninth.origin.x, first.origin.x);
}

#[gpui_kit::test]
fn keys_move_and_type(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, &"a".repeat(40), cx);
    let editor = hex(&workspace, cx);
    editor.update(cx, |editor, cx| editor.move_to(0, false, cx));
    cx.simulate_keystrokes("right right down");
    assert_eq!(caret(&editor, cx), (18, 18));
    cx.simulate_keystrokes("shift-left");
    assert_eq!(caret(&editor, cx), (18, 17));
    cx.simulate_keystrokes("home");
    assert_eq!(caret(&editor, cx), (16, 16));
    cx.simulate_keystrokes("end up");
    assert_eq!(caret(&editor, cx), (15, 15));
    cx.simulate_keystrokes("4 2");
    assert_eq!(&active_text(&workspace, cx)[15..17], "Ba");
    cx.simulate_keystrokes("tab");
    assert!(editor.read_with(cx, |editor, _| editor.in_text));
    cx.simulate_keystrokes("z insert");
    assert_eq!(&active_text(&workspace, cx)[16..18], "za");
    assert!(editor.read_with(cx, |editor, _| !editor.overwrite));
    cx.simulate_keystrokes("pagedown");
    assert_eq!(caret(&editor, cx), (40, 40));
}

#[gpui_kit::test]
fn tab_and_shift_tab_switch_columns_and_stay_in_the_editor(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "abc", cx);
    // Not in a hex view: nothing to switch.
    let error = run(&workspace, Invocation::new("hex.switch-column"), cx).unwrap_err();
    assert!(error.to_string().contains("not shown in HEX"), "{error}");
    let editor = hex(&workspace, cx);
    editor.update(cx, |editor, cx| editor.move_to(0, false, cx));
    // Half a byte typed is dropped on the way to the text column.
    cx.simulate_keystrokes("4 tab");
    assert!(editor.read_with(cx, |editor, _| editor.in_text));
    cx.simulate_keystrokes("shift-tab");
    assert!(editor.read_with(cx, |editor, _| !editor.in_text));
    // Neither key reached the document: no tab typed, no line outdented.
    assert_eq!(active_text(&workspace, cx), "abc");
    run(&workspace, Invocation::new("hex.switch-column"), cx).unwrap();
    assert!(editor.read_with(cx, |editor, _| editor.in_text));
}
