use birchpad_commands::Invocation;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use crate::workspace::{active_text, open_workspace};
use anyhow::Result;
use birchpad::workspace::Workspace;

fn run(workspace: &Entity<Workspace>, id: &str, cx: &mut VisualTestContext) -> Result<()> {
    let result = workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&Invocation::new(id), window, cx)
    });
    cx.run_until_parked();
    result
}

fn type_text(workspace: &Entity<Workspace>, text: &str, cx: &mut VisualTestContext) {
    let invocation = Invocation::with_args("edit.insert-text", json!({ "text": text }));
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.dispatch(&invocation, window, cx)
        })
        .unwrap();
    cx.run_until_parked();
}

/// The text with the platform's line break and indentation (CRLF and tabs on Windows) made
/// LF and four spaces.
fn plain(text: String) -> String {
    text.replace("\r\n", "\n").replace('\t', "    ")
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

fn select(
    workspace: &Entity<Workspace>,
    range: std::ops::Range<usize>,
    cx: &mut VisualTestContext,
) {
    workspace.update(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.update(cx, |view, cx| view.select_range(range, cx));
    });
}

#[gpui_kit::test]
fn formats_and_compresses_the_whole_json_in_one_undo_step(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, r#"{"a":[1,{}],"b":"x"}"#, cx);
    run(&workspace, "json.format", cx).unwrap();
    assert_eq!(
        plain(active_text(&workspace, cx)),
        "{\n    \"a\": [\n        1,\n        {}\n    ],\n    \"b\": \"x\"\n}"
    );
    assert_eq!(selection(&workspace, cx), (0, 0));
    run(&workspace, "json.minify", cx).unwrap();
    assert_eq!(active_text(&workspace, cx), r#"{"a":[1,{}],"b":"x"}"#);
    run(&workspace, "edit.undo", cx).unwrap();
    assert!(plain(active_text(&workspace, cx)).starts_with("{\n"));
    run(&workspace, "json.validate", cx).unwrap();
}

#[gpui_kit::test]
fn works_on_the_selection_only(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "x = <a><b/></a>;", cx);
    select(&workspace, 4..15, cx);
    run(&workspace, "xml.pretty-print", cx).unwrap();
    let text = active_text(&workspace, cx);
    assert_eq!(plain(text.clone()), "x = <a>\n    <b/>\n</a>;");
    assert_eq!(selection(&workspace, cx), (4, text.len() - 1));
    run(&workspace, "xml.linearize", cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "x = <a><b/></a>;");
    run(&workspace, "xml.check", cx).unwrap();
}

#[gpui_kit::test]
fn goes_to_the_error_and_changes_nothing(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "{\n  \"a\": 1,\n}", cx);
    for id in ["json.format", "json.minify", "json.validate"] {
        run(&workspace, "cursor.document-start", cx).unwrap();
        let error = run(&workspace, id, cx).unwrap_err();
        assert_eq!(
            error.to_string(),
            "line 3, column 1: expected a key in quotes",
            "{id}"
        );
        assert_eq!(selection(&workspace, cx), (12, 12));
    }
    assert_eq!(active_text(&workspace, cx), "{\n  \"a\": 1,\n}");

    run(&workspace, "edit.select-all", cx).unwrap();
    run(&workspace, "edit.delete", cx).unwrap();
    type_text(&workspace, "<a>\n<b></a>", cx);
    let error = run(&workspace, "xml.check", cx).unwrap_err();
    assert!(
        error.to_string().starts_with("line 2, column 4:"),
        "{error}"
    );
    assert!(run(&workspace, "xml.linearize", cx).is_err());
    assert_eq!(active_text(&workspace, cx), "<a>\n<b></a>");
}

#[gpui_kit::test]
fn an_empty_document_is_not_json_or_xml(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    assert!(run(&workspace, "json.format", cx).is_err());
    assert!(run(&workspace, "xml.pretty-print", cx).is_err());
    assert_eq!(active_text(&workspace, cx), "");
    // What is already formatted stays as it is.
    type_text(&workspace, "[]", cx);
    run(&workspace, "json.format", cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "[]");
}
