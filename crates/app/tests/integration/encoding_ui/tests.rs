use std::path::{Path, PathBuf};

use birchpad_core::{Encoding, LineEnding};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use crate::workspace::{active_text, open_workspace, secondary};
use birchpad::encoding_ui::*;
use birchpad::workspace::Workspace;
use birchpad_commands::Invocation;
use birchpad_core::Format;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../io/tests/fixtures")
        .join(name)
}

fn run(workspace: &Entity<Workspace>, invocation: Invocation, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&invocation, window, cx).unwrap();
    });
    cx.run_until_parked();
}

fn active_format(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Format {
    workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.read(cx).buffer.read(cx).doc().format()
    })
}

fn open(workspace: &Entity<Workspace>, name: &str, cx: &mut VisualTestContext) {
    let path = fixture(name);
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(&path, window, cx)
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn opens_files_in_the_background_with_their_format(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    open(&workspace, "koi8-r-crlf.txt", cx);
    let format = active_format(&workspace, cx);
    assert_eq!(format.encoding, Encoding::Legacy("KOI8-R"));
    assert_eq!(format.line_ending, LineEnding::CrLf);
    assert!(active_text(&workspace, cx).starts_with("Съешь"));
    // The untouched "new 1" was replaced; opening the file again switches to its tab.
    open(&workspace, "koi8-r-crlf.txt", cx);
    assert_eq!(
        crate::workspace::tab_names(&workspace, cx),
        ["koi8-r-crlf.txt"]
    );
}

#[gpui_kit::test]
fn convert_to_is_undoable(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    open(&workspace, "utf-8-lf.txt", cx);
    run(
        &workspace,
        Invocation::with_args("encoding.convert-to", json!({ "encoding": "utf-16le-bom" })),
        cx,
    );
    let format = active_format(&workspace, cx);
    assert_eq!((format.encoding, format.bom), (Encoding::Utf16Le, true));

    cx.simulate_keystrokes(&secondary("z"));
    let format = active_format(&workspace, cx);
    assert_eq!((format.encoding, format.bom), (Encoding::Utf8, false));
    let modified = workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.read(cx).buffer.read(cx).is_modified()
    });
    assert!(
        !modified,
        "undoing the conversion returns to the saved state"
    );
}

#[gpui_kit::test]
fn encode_in_rereads_the_bytes_and_bad_bytes_make_it_read_only(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    open(&workspace, "invalid-utf8.txt", cx);
    assert!(
        !active_format(&workspace, cx).encoding.is_unicode(),
        "detected as ANSI"
    );

    run(
        &workspace,
        Invocation::with_args("encoding.encode-in", json!({ "encoding": "utf-8" })),
        cx,
    );
    assert_eq!(active_format(&workspace, cx).encoding, Encoding::Utf8);
    let before = active_text(&workspace, cx);
    assert!(before.contains('\u{FFFD}'));
    cx.simulate_input("typed");
    cx.simulate_keystrokes("backspace");
    assert_eq!(active_text(&workspace, cx), before, "read-only");

    let convert = workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(
            &Invocation::with_args("encoding.convert-to", json!({ "encoding": "utf-8-bom" })),
            window,
            cx,
        )
    });
    assert!(convert.is_err());
}

#[gpui_kit::test]
fn files_that_do_not_decode_can_be_edited_anyway(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.txt");
    std::fs::write(&path, b"ok \xC3( caf\xE9\n").unwrap();
    let (workspace, cx) = open_workspace(cx);
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(&path, window, cx)
    });
    cx.run_until_parked();
    run(
        &workspace,
        Invocation::with_args("encoding.encode-in", json!({ "encoding": "utf-8" })),
        cx,
    );
    let changes = workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.read(cx).buffer.read(cx).decode_changes().clone()
    });
    assert_eq!(
        changes.first.iter().map(|c| c.offset).collect::<Vec<_>>(),
        [3, 9]
    );

    let edit_anyway = Invocation::new("encoding.edit-anyway");
    run(&workspace, edit_anyway.clone(), cx);
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.simulate_input("typed");
    assert_eq!(active_text(&workspace, cx), "ok \u{FFFD}( caf\u{FFFD}\n");

    run(&workspace, edit_anyway.clone(), cx);
    cx.simulate_prompt_answer("Edit Anyway");
    cx.run_until_parked();
    cx.simulate_input("X");
    cx.simulate_keystrokes(&secondary("s"));
    cx.run_until_parked();
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"Xok \xEF\xBF\xBD( caf\xEF\xBF\xBD\n",
        "only the bytes that did not decode changed, besides the edit"
    );
    let decoded = workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&edit_anyway, window, cx)
    });
    assert!(decoded.is_err(), "nothing to accept any more");
}

#[test]
fn the_warning_lists_the_bytes_that_change() {
    let changes = birchpad_io::byte_changes(b"a\xFA\x5Bb\x82", 0, Encoding::Legacy("Shift_JIS"));
    assert_eq!(
        describe_changes(&changes, "Shift_JIS"),
        "Saving writes the text in Shift_JIS again, which changes 2 places in the file:\n\
             • offset 1: FA 5B (∵) becomes 81 E6\n\
             • offset 4: 82 (\u{FFFD}) cannot be written in Shift_JIS\n\
             \n\
             Saving is refused while the text has characters Shift_JIS cannot write: replace \
             them, or convert the document to UTF-8.\n\
             \n\
             Everything else is saved as it was, apart from your edits."
    );
}

#[gpui_kit::test]
fn eol_conversion_from_the_menu(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    open(&workspace, "mixed-eol.txt", cx);
    run(
        &workspace,
        Invocation::with_args("edit.convert-eol", json!({ "eol": "lf" })),
        cx,
    );
    assert_eq!(active_text(&workspace, cx), "crlf\nlf\ncr\nlast");
    assert_eq!(active_format(&workspace, cx).line_ending, LineEnding::Lf);
    let checked = is_current(
        &Invocation::with_args("edit.convert-eol", json!({ "eol": "lf" })),
        active_format(&workspace, cx),
        Encoding::Legacy("windows-1252"),
    );
    assert!(checked);
}

/// The banner over a file that does not decode: its buttons, and what it says.
#[gpui_kit::test]
fn the_banner_of_a_file_that_does_not_decode(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let malformed = dir.path().join("bad.txt");
    std::fs::write(&malformed, b"ok \xC3(\n").unwrap();
    let (workspace, cx) = open_workspace(cx);
    let open_as = |path: &Path, encoding: &str, cx: &mut VisualTestContext| {
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_path(path, window, cx)
        });
        cx.run_until_parked();
        run(
            &workspace,
            Invocation::with_args("encoding.encode-in", json!({ "encoding": encoding })),
            cx,
        );
    };
    open_as(&malformed, "utf-8", cx);
    let click = |selector: &'static str, cx: &mut VisualTestContext| {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        crate::workspace::click_on(selector, gpui_kit::MouseButton::Left, cx);
    };
    // Reopen With Encoding lists the encodings.
    click("banner-reopen", cx);
    assert!(cx.debug_bounds("banner-decoding").is_some());
    cx.simulate_keystrokes("escape");
    // Edit Anyway asks first; cancelled, the file stays read-only.
    click("banner-edit-anyway", cx);
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert!(cx.debug_bounds("banner-decoding").is_some());
    click("banner-edit-anyway", cx);
    cx.simulate_prompt_answer("Edit Anyway");
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("banner-decoding").is_none(), "editable now");

    // Bytes that decode, but would change when saved (Shift_JIS's IBM extensions).
    let shift_jis = dir.path().join("sjis.txt");
    std::fs::write(&shift_jis, b"ab\xFA\x5B").unwrap();
    open_as(&shift_jis, "shift_jis", cx);
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("banner-decoding").is_some());
}
