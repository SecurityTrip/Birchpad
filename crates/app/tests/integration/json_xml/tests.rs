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

fn run_with(
    workspace: &Entity<Workspace>,
    id: &str,
    args: serde_json::Value,
    cx: &mut VisualTestContext,
) -> Result<()> {
    let result = workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&Invocation::with_args(id, args), window, cx)
    });
    cx.run_until_parked();
    result
}

fn selected_text(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    let (anchor, head) = selection(workspace, cx);
    active_text(workspace, cx)[anchor.min(head)..anchor.max(head)].to_owned()
}

fn clipboard(cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .unwrap_or_default()
    })
}

fn viewer(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> Entity<birchpad::panels::JsonViewer> {
    workspace.read_with(cx, |workspace, _| {
        workspace
            .docks
            .view(birchpad::panels::PanelKind::JsonViewer)
            .unwrap()
            .clone()
            .downcast::<birchpad::panels::JsonViewer>()
            .unwrap()
    })
}

const STORE: &str =
    r#"{"store": {"book": [{"title": "Rust", "price": 30}, {"title": "Go \"2\""}]}, "open": true}"#;

#[gpui_kit::test]
fn json_viewer_shows_the_tree_and_finds_paths(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, STORE, cx);
    run(&workspace, "panel.json-viewer", cx).unwrap();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let viewer = viewer(&workspace, cx);
    let rows = viewer.update(cx, |viewer, cx| {
        viewer.refresh(cx);
        let parsed = viewer.parsed.as_ref().unwrap();
        let tree = parsed.tree.as_ref().unwrap();
        birchpad::panels::json_viewer::rows(tree, &viewer.collapsed)
            .iter()
            .map(|row| birchpad::panels::json_viewer::label(tree, row.node, &parsed.text))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        rows,
        [
            "{2}",
            "store: {1}",
            "book: [2]",
            "[0]: {2}",
            "title: \"Rust\"",
            "price: 30",
            "[1]: {1}",
            "title: \"Go \\\"2\\\"\"",
            "open: true"
        ]
    );
    assert!(cx.debug_bounds("json-viewer-0").is_some());

    run_with(
        &workspace,
        "json.find-path",
        json!({ "path": "$.store.book[1].title" }),
        cx,
    )
    .unwrap();
    assert_eq!(selected_text(&workspace, cx), "\"Go \\\"2\\\"\"");
    run(&workspace, "json.copy-value", cx).unwrap();
    assert_eq!(clipboard(cx), "Go \"2\"");
    run(&workspace, "json.copy-key", cx).unwrap();
    assert_eq!(clipboard(cx), "title");
    run(&workspace, "json.copy-path", cx).unwrap();
    assert_eq!(clipboard(cx), "$.store.book[1].title");

    // A path that names nothing, or does not read: said so, nothing chosen changes.
    let error = run_with(
        &workspace,
        "json.find-path",
        json!({ "path": "store.pen" }),
        cx,
    )
    .unwrap_err();
    assert!(error.to_string().contains("no value"), "{error}");
    assert!(
        run_with(
            &workspace,
            "json.find-path",
            json!({ "path": "store[" }),
            cx
        )
        .is_err()
    );
    assert!(run_with(&workspace, "json.find-path", json!({}), cx).is_err());
}

#[gpui_kit::test]
fn json_viewer_folds_and_follows_edits(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, STORE, cx);
    run(&workspace, "panel.json-viewer", cx).unwrap();
    let viewer = viewer(&workspace, cx);
    let count = |viewer: &Entity<birchpad::panels::JsonViewer>, cx: &mut VisualTestContext| {
        viewer.update(cx, |viewer, cx| {
            viewer.refresh(cx);
            match &viewer.parsed.as_ref().unwrap().tree {
                Ok(tree) => birchpad::panels::json_viewer::rows(tree, &viewer.collapsed).len(),
                Err(_) => 0,
            }
        })
    };
    assert_eq!(count(&viewer, cx), 9);
    // Folding "store" hides what is below it; it stays folded through an edit.
    viewer.update(cx, |viewer, cx| viewer.toggle(1, cx));
    assert_eq!(count(&viewer, cx), 3);
    run(&workspace, "edit.select-all", cx).unwrap();
    type_text(
        &workspace,
        r#"{"store": {"book": []}, "more": 1, "open": false}"#,
        cx,
    );
    assert_eq!(count(&viewer, cx), 4);
    // Text that is not JSON shows where it breaks, and no rows.
    type_text(&workspace, "}", cx);
    assert_eq!(count(&viewer, cx), 0);
    let shown = viewer.read_with(cx, |viewer, _| {
        viewer.parsed.as_ref().unwrap().tree.is_err()
    });
    assert!(shown);
}

#[gpui_kit::test]
fn copies_the_json_value_at_the_caret_without_the_viewer(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, STORE, cx);
    // The caret in "Rust".
    let at = STORE.find("Rust").unwrap() + 1;
    select(&workspace, at..at, cx);
    run(&workspace, "json.copy-value", cx).unwrap();
    assert_eq!(clipboard(cx), "Rust");
    run(&workspace, "json.copy-path", cx).unwrap();
    assert_eq!(clipboard(cx), "$.store.book[0].title");
    run(&workspace, "json.copy-key", cx).unwrap();
    assert_eq!(clipboard(cx), "title");
    // In an array element: its index is its key.
    let at = STORE.find("{\"title\": \"Go").unwrap();
    select(&workspace, at..at, cx);
    run(&workspace, "json.copy-key", cx).unwrap();
    assert_eq!(clipboard(cx), "1");
    // Not JSON: the reason.
    run(&workspace, "edit.select-all", cx).unwrap();
    type_text(&workspace, "{oops", cx);
    assert!(run(&workspace, "json.copy-value", cx).is_err());
}

#[gpui_kit::test]
fn sorts_json_keys(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, r#"{"b":1,"a":{"d":[2,1],"c":0}}"#, cx);
    run(&workspace, "json.sort-keys", cx).unwrap();
    assert_eq!(
        plain(active_text(&workspace, cx)),
        "{\n    \"a\": {\n        \"c\": 0,\n        \"d\": [\n            2,\n            1\n        ]\n    },\n    \"b\": 1\n}"
    );
    run(&workspace, "edit.undo", cx).unwrap();
    assert_eq!(
        active_text(&workspace, cx),
        r#"{"b":1,"a":{"d":[2,1],"c":0}}"#
    );
    run(&workspace, "edit.select-all", cx).unwrap();
    type_text(&workspace, "[1,", cx);
    assert!(run(&workspace, "json.sort-keys", cx).is_err());
    assert_eq!(active_text(&workspace, cx), "[1,");
}

const LIBRARY: &str = "<library><book id=\"1\"><title>Rust</title></book><book id=\"2\"><title>Go</title></book></library>";

#[gpui_kit::test]
fn evaluates_xpath_and_selects_the_first_node(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, LIBRARY, cx);
    run_with(
        &workspace,
        "xml.xpath",
        json!({ "expression": "//book[@id='2']/title" }),
        cx,
    )
    .unwrap();
    assert_eq!(selected_text(&workspace, cx), "<title>Go</title>");
    let last = cx.update(|_, cx| {
        let last = cx.global::<birchpad::json_xml::LastXPath>();
        (last.expression.clone(), last.result.clone())
    });
    assert_eq!(last.0, "//book[@id='2']/title");
    assert!(matches!(
        last.1,
        Some(Ok(birchpad_tools::xpath::XPathResult::Nodes(ref nodes))) if nodes.len() == 1
    ));
    // A value selects nothing; an error is said and kept for the dialog.
    run_with(
        &workspace,
        "xml.xpath",
        json!({ "expression": "count(//book)" }),
        cx,
    )
    .unwrap();
    let error = run_with(
        &workspace,
        "xml.xpath",
        json!({ "expression": "//book[" }),
        cx,
    )
    .unwrap_err();
    assert!(error.to_string().contains("too soon"), "{error}");
    let kept = cx.update(|_, cx| {
        matches!(
            cx.global::<birchpad::json_xml::LastXPath>().result,
            Some(Err(_))
        )
    });
    assert!(kept);
}

#[gpui_kit::test]
fn the_xpath_dialog_evaluates_and_goes_to_results(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, LIBRARY, cx);
    run(&workspace, "xml.xpath", cx).unwrap();
    let dialog = birchpad::json_xml::XPATH_OPENED
        .with(|opened| opened.borrow().clone().and_then(|weak| weak.upgrade()))
        .unwrap();
    dialog.update_in(cx, |dialog, window, cx| {
        dialog
            .input
            .update(cx, |input, cx| input.set_value("//title", window, cx));
        dialog.evaluate(window, cx);
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("xpath-result-1").is_some());
    crate::workspace::click_on("xpath-result-1", gpui_kit::MouseButton::Left, cx);
    assert_eq!(selected_text(&workspace, cx), "<title>Go</title>");
}

#[gpui_kit::test]
fn validates_against_the_dtd_the_document_declares(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dtd = "<!DOCTYPE list [<!ELEMENT list (item+)><!ELEMENT item (#PCDATA)><!ATTLIST item id ID #REQUIRED>]>";
    type_text(
        &workspace,
        &format!("{dtd}<list><item id=\"a\">x</item></list>"),
        cx,
    );
    run(&workspace, "xml.validate", cx).unwrap();
    run(&workspace, "edit.select-all", cx).unwrap();
    type_text(
        &workspace,
        &format!("{dtd}<list><item>x</item><item id=\"b\"><b/></item></list>"),
        cx,
    );
    let error = run(&workspace, "xml.validate", cx).unwrap_err().to_string();
    assert!(error.contains("needs the attribute id"), "{error}");
    assert!(error.contains("2 more"), "{error}");
    // The first problem's tag is selected.
    assert_eq!(selected_text(&workspace, cx), "<item>");
    let problems = cx.update(|_, cx| {
        cx.global::<birchpad::json_xml::LastValidation>()
            .problems
            .len()
    });
    assert_eq!(problems, 3);
}

#[gpui_kit::test]
fn validates_against_a_dtd_or_schema_on_disk(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("note.dtd"), "<!ELEMENT note (#PCDATA)>").unwrap();
    std::fs::write(
        dir.path().join("note.xsd"),
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:element name="note" type="xs:int"/></xs:schema>"#,
    )
    .unwrap();
    // The DTD its SYSTEM identifier names, next to the document.
    let with_dtd = dir.path().join("with-dtd.xml");
    std::fs::write(
        &with_dtd,
        "<!DOCTYPE note SYSTEM \"note.dtd\"><note>hi</note>",
    )
    .unwrap();
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(&with_dtd, window, cx)
    });
    cx.run_until_parked();
    run(&workspace, "xml.validate", cx).unwrap();
    // The schema its root names.
    let with_schema = dir.path().join("with-schema.xml");
    std::fs::write(
        &with_schema,
        r#"<note xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:noNamespaceSchemaLocation="note.xsd">hi</note>"#,
    )
    .unwrap();
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(&with_schema, window, cx)
    });
    cx.run_until_parked();
    let error = run(&workspace, "xml.validate", cx).unwrap_err().to_string();
    assert!(error.contains("is not a int"), "{error}");
    // A schema given: it wins.
    let other = dir.path().join("other.xsd");
    std::fs::write(
        &other,
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:element name="note" type="xs:string"/></xs:schema>"#,
    )
    .unwrap();
    run_with(&workspace, "xml.validate", json!({ "schema": other }), cx).unwrap();
    // A schema that is not there, or not on disk.
    let missing = dir.path().join("missing.xsd");
    let error = run_with(&workspace, "xml.validate", json!({ "schema": missing }), cx).unwrap_err();
    assert!(format!("{error:#}").contains("missing.xsd"), "{error:#}");
    run(&workspace, "edit.select-all", cx).unwrap();
    type_text(
        &workspace,
        r#"<note xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:noNamespaceSchemaLocation="https://example.com/n.xsd"/>"#,
        cx,
    );
    let error = run(&workspace, "xml.validate", cx).unwrap_err().to_string();
    assert!(error.contains("not a file"), "{error}");
}

#[gpui_kit::test]
fn validate_asks_for_a_schema_when_the_document_names_none(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "<note>5</note>", cx);
    let dir = tempfile::tempdir().unwrap();
    let schema = dir.path().join("note.xsd");
    std::fs::write(
        &schema,
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:element name="note" type="xs:int"/></xs:schema>"#,
    )
    .unwrap();
    run(&workspace, "xml.validate", cx).unwrap();
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(move |_| Some(vec![schema]));
    cx.run_until_parked();
    let problems = cx.update(|_, cx| {
        cx.global::<birchpad::json_xml::LastValidation>()
            .problems
            .len()
    });
    assert_eq!(problems, 0);
}

#[gpui_kit::test]
fn escapes_and_unescapes_xml(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "a < b & \"c\" 'd' > e", cx);
    run(&workspace, "xml.escape", cx).unwrap();
    assert_eq!(
        active_text(&workspace, cx),
        "a &lt; b &amp; &quot;c&quot; &apos;d&apos; &gt; e"
    );
    run(&workspace, "xml.unescape", cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "a < b & \"c\" 'd' > e");
    // Only the selection, when there is one.
    select(&workspace, 0..3, cx);
    run(&workspace, "xml.escape", cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "a &lt; b & \"c\" 'd' > e");
}

#[gpui_kit::test]
fn tag_auto_close_is_the_close_tags_setting(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    cx.update(|_, cx| {
        cx.global_mut::<birchpad::app_state::AppState>()
            .paths
            .user_settings = Some(dir.path().join("settings.toml"));
    });
    let close_tags = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            birchpad::app_state::AppState::global(cx)
                .settings
                .completion
                .close_tags
        })
    };
    assert!(!close_tags(cx));
    run(&workspace, "xml.tag-auto-close", cx).unwrap();
    assert!(close_tags(cx));
    let written = std::fs::read_to_string(dir.path().join("settings.toml")).unwrap();
    assert!(written.contains("close-tags = true"), "{written}");
    run(&workspace, "xml.tag-auto-close", cx).unwrap();
    assert!(!close_tags(cx));
}
