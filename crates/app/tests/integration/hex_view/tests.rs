use birchpad_commands::Invocation;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use crate::workspace::{active_text, open_workspace, tab_names};
use anyhow::Result;
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

/// Replaces the whole text of the active document.
fn set_text(workspace: &Entity<Workspace>, text: &str, cx: &mut VisualTestContext) {
    run(workspace, Invocation::new("edit.select-all"), cx).unwrap();
    type_text(workspace, text, cx);
}

#[gpui_kit::test]
fn shows_the_bytes_and_applies_them_back(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "Hi мир", cx);
    run(&workspace, Invocation::new("hex.view"), cx).unwrap();
    assert_eq!(tab_names(&workspace, cx), ["new 1", "new 2"]);
    let dump = active_text(&workspace, cx);
    assert!(
        dump.starts_with("00000000  48 69 20 D0 BC D0 B8 D1  80"),
        "{dump}"
    );
    // Change "Hi" to "Ho" and take a byte off the end of "р": the bytes are no longer UTF-8.
    set_text(&workspace, "00000000 48 6F 20 D0 BC D0 B8 D1", cx);
    let error = run(&workspace, Invocation::new("hex.apply"), cx).unwrap_err();
    assert!(
        error.to_string().contains("not UTF-8 text (at byte 7)"),
        "{error}"
    );
    set_text(&workspace, "00000000 48 6F 20 D0 BC D0 B8 D1 80 21", cx);
    run(&workspace, Invocation::new("hex.apply"), cx).unwrap();
    // Back in the document, changed as one undo step.
    assert_eq!(active_text(&workspace, cx), "Ho мир!");
    run(&workspace, Invocation::new("edit.undo"), cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "Hi мир");
}

#[gpui_kit::test]
fn shows_the_bytes_of_the_documents_encoding(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "A", cx);
    run(
        &workspace,
        Invocation::with_args("encoding.convert-to", json!({ "encoding": "utf-16le-bom" })),
        cx,
    )
    .unwrap();
    run(&workspace, Invocation::new("hex.view"), cx).unwrap();
    assert!(active_text(&workspace, cx).starts_with("00000000  FF FE 41 00 "));
    set_text(&workspace, "0 FF FE 42 00 43 00", cx);
    run(&workspace, Invocation::new("hex.apply"), cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "BC");
}

#[gpui_kit::test]
fn an_empty_document_has_an_empty_dump(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    run(&workspace, Invocation::new("hex.view"), cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "");
    type_text(&workspace, "0 41", cx);
    run(&workspace, Invocation::new("hex.apply"), cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "A");
}

#[gpui_kit::test]
fn refuses_what_is_not_a_hex_view(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "text", cx);
    let error = run(&workspace, Invocation::new("hex.apply"), cx).unwrap_err();
    assert!(error.to_string().contains("not a hex dump"), "{error}");

    run(&workspace, Invocation::new("hex.view"), cx).unwrap();
    set_text(&workspace, "0 4G", cx);
    let error = run(&workspace, Invocation::new("hex.apply"), cx).unwrap_err();
    assert!(error.to_string().contains("line 1"), "{error}");

    // The document it shows is closed: an unchanged one, which closes without asking.
    run(&workspace, Invocation::new("file.new"), cx).unwrap();
    run(&workspace, Invocation::new("hex.view"), cx).unwrap();
    type_text(&workspace, "0 41", cx);
    run(&workspace, Invocation::new("view.previous-tab"), cx).unwrap();
    run(&workspace, Invocation::new("file.close"), cx).unwrap();
    assert_eq!(tab_names(&workspace, cx), ["new 1", "new 2", "new 4"]);
    let error = run(&workspace, Invocation::new("hex.apply"), cx).unwrap_err();
    assert!(error.to_string().contains("closed"), "{error}");
}
