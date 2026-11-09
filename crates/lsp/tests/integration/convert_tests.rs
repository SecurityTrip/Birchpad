use std::path::PathBuf;

use birchpad_core::Rope;
use birchpad_lsp::PositionEncoding;
use birchpad_lsp::convert::*;
use birchpad_lsp::lsp_types::{
    CompletionResponse, Diagnostic, DiagnosticSeverity, GotoDefinitionResponse, HoverContents,
    Location, Position, Range, TextEdit, WorkspaceEdit,
};
use serde_json::json;

fn range(a: (u32, u32), b: (u32, u32)) -> Range {
    Range::new(Position::new(a.0, a.1), Position::new(b.0, b.1))
}

fn edit(a: (u32, u32), b: (u32, u32), text: &str) -> TextEdit {
    TextEdit::new(range(a, b), text.to_owned())
}

/// A file URI and its path, for this platform.
fn file(name: &str) -> (String, PathBuf) {
    if cfg!(windows) {
        (
            format!("file:///C:/src/{name}"),
            PathBuf::from(format!(r"C:\src\{name}")),
        )
    } else {
        (
            format!("file:///src/{name}"),
            PathBuf::from(format!("/src/{name}")),
        )
    }
}

#[test]
fn text_edits_become_byte_ranges_in_order() {
    let text = Rope::from_str("fn ё() {\n}\n");
    let edits = text_edits(
        &text,
        &[
            edit((1, 0), (1, 1), "};"),
            edit((0, 3), (0, 4), "main"),
            // Two inserts at the same place keep their order.
            edit((0, 0), (0, 0), "pub "),
            edit((0, 0), (0, 0), "async "),
        ],
        PositionEncoding::Utf16,
    )
    .unwrap();
    let ranges: Vec<_> = edits
        .iter()
        .map(|e| (e.range.clone(), e.text.as_str()))
        .collect();
    assert_eq!(
        ranges,
        [
            (0..0, "pub "),
            (0..0, "async "),
            (3..5, "main"),
            (10..11, "};"),
        ]
    );
    // None at all.
    assert!(
        text_edits(&text, &[], PositionEncoding::Utf8)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn overlapping_edits_are_refused() {
    let text = Rope::from_str("abcdef");
    let result = text_edits(
        &text,
        &[edit((0, 0), (0, 3), "x"), edit((0, 2), (0, 4), "y")],
        PositionEncoding::Utf16,
    );
    assert_eq!(result, Err(ConvertError::Overlapping));
    // Touching edits do not overlap.
    let touching = text_edits(
        &text,
        &[edit((0, 3), (0, 4), "y"), edit((0, 0), (0, 3), "x")],
        PositionEncoding::Utf16,
    )
    .unwrap();
    assert_eq!(touching.len(), 2);
}

#[test]
fn workspace_edits_by_file() {
    let (a_uri, a) = file("a.rs");
    let (b_uri, b) = file("b.rs");
    let mut by_file = serde_json::Map::new();
    by_file.insert(
        b_uri.clone(),
        json!([{ "range": range((0, 0), (0, 1)), "newText": "B" }]),
    );
    by_file.insert(
        a_uri.clone(),
        json!([{ "range": range((1, 0), (1, 1)), "newText": "A" }]),
    );
    let changes: WorkspaceEdit = serde_json::from_value(json!({ "changes": by_file })).unwrap();
    let files = workspace_edits(&changes).unwrap();
    assert_eq!(
        files.keys().cloned().collect::<Vec<_>>(),
        [a.clone(), b.clone()]
    );
    assert_eq!(files[&a][0].new_text, "A");
    // documentChanges, with an annotated edit, and two edits of one file merged.
    let documents: WorkspaceEdit = serde_json::from_value(json!({
        "documentChanges": [
            { "textDocument": { "uri": a_uri, "version": 3 },
              "edits": [{ "range": range((0, 0), (0, 1)), "newText": "1" }] },
            { "textDocument": { "uri": a_uri, "version": 3 },
              "edits": [{ "range": range((2, 0), (2, 1)), "newText": "2", "annotationId": "x" }] },
        ],
        // Left alone when documentChanges are given.
        "changes": serde_json::Map::from_iter([(b_uri, json!([]))]),
    }))
    .unwrap();
    let files = workspace_edits(&documents).unwrap();
    assert_eq!(files.len(), 1);
    let texts: Vec<_> = files[&a].iter().map(|e| e.new_text.as_str()).collect();
    assert_eq!(texts, ["1", "2"]);
    // An empty edit changes nothing.
    assert!(
        workspace_edits(&WorkspaceEdit::default())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn workspace_edits_that_change_files_or_leave_them_are_refused() {
    let (a_uri, _) = file("a.rs");
    let operations: WorkspaceEdit = serde_json::from_value(json!({
        "documentChanges": [
            { "textDocument": { "uri": a_uri, "version": null },
              "edits": [{ "range": range((0, 0), (0, 1)), "newText": "1" }] },
            { "kind": "rename", "oldUri": a_uri, "newUri": file("c.rs").0 },
        ]
    }))
    .unwrap();
    assert_eq!(
        workspace_edits(&operations),
        Err(ConvertError::ResourceOperation)
    );
    // Edits only, as operations: accepted.
    let edits_only: WorkspaceEdit = serde_json::from_value(json!({
        "documentChanges": [
            { "textDocument": { "uri": a_uri, "version": null },
              "edits": [{ "range": range((0, 0), (0, 1)), "newText": "1" }] },
        ]
    }))
    .unwrap();
    assert_eq!(workspace_edits(&edits_only).unwrap().len(), 1);
    let elsewhere: WorkspaceEdit = serde_json::from_value(json!({
        "changes": { "untitled:Untitled-1": [] }
    }))
    .unwrap();
    assert_eq!(
        workspace_edits(&elsewhere),
        Err(ConvertError::NotAFile("untitled:Untitled-1".into()))
    );
    assert!(
        ConvertError::NotAFile("x:y".into())
            .to_string()
            .contains("x:y")
    );
}

#[test]
fn definitions_of_every_shape() {
    let (a_uri, a) = file("a.rs");
    let location: Location = serde_json::from_value(json!({
        "uri": a_uri, "range": range((1, 2), (1, 3))
    }))
    .unwrap();
    let scalar = definitions(Some(GotoDefinitionResponse::Scalar(location.clone())));
    assert_eq!(
        scalar,
        [FileRange {
            path: a.clone(),
            range: range((1, 2), (1, 3))
        }]
    );
    let other: Location =
        serde_json::from_value(json!({ "uri": "https://x/y", "range": range((0, 0), (0, 0)) }))
            .unwrap();
    let array = definitions(Some(GotoDefinitionResponse::Array(vec![
        location.clone(),
        other,
    ])));
    assert_eq!(array.len(), 1, "the one not in a file is left out");
    let links: GotoDefinitionResponse = serde_json::from_value(json!([{
        "targetUri": a_uri,
        "targetRange": range((0, 0), (5, 0)),
        "targetSelectionRange": range((1, 4), (1, 8)),
    }]))
    .unwrap();
    assert_eq!(definitions(Some(links))[0].range, range((1, 4), (1, 8)));
    assert!(definitions(None).is_empty());
    assert!(definitions(Some(GotoDefinitionResponse::Array(vec![]))).is_empty());
}

#[test]
fn references_are_sorted_and_unique() {
    let (a_uri, a) = file("a.rs");
    let (b_uri, b) = file("b.rs");
    let found: Vec<Location> = serde_json::from_value(json!([
        { "uri": b_uri, "range": range((0, 0), (0, 1)) },
        { "uri": a_uri, "range": range((3, 1), (3, 2)) },
        { "uri": a_uri, "range": range((1, 5), (1, 6)) },
        { "uri": a_uri, "range": range((1, 5), (1, 6)) },
    ]))
    .unwrap();
    let sorted = references(Some(found));
    let places: Vec<_> = sorted
        .iter()
        .map(|place| (place.path.clone(), place.range.start.line))
        .collect();
    assert_eq!(places, [(a.clone(), 1), (a, 3), (b, 0)]);
    assert!(references(None).is_empty());
    assert!(references(Some(vec![])).is_empty());
}

#[test]
fn completion_items_as_entries() {
    let response: CompletionResponse = serde_json::from_value(json!({
        "isIncomplete": false,
        "items": [
            { "label": "zeta", "sortText": "1" },
            { "label": "alpha", "detail": "fn alpha()\nmore", "sortText": "2",
              "insertText": "alpha()" },
            { "label": "beta", "textEdit": { "range": range((0, 0), (0, 2)), "newText": "beta_x" },
              "filterText": "bx", "detail": "  " },
            { "label": "gamma", "textEdit": {
                "insert": range((0, 0), (0, 1)), "replace": range((0, 0), (0, 3)),
                "newText": "gamma" } },
            { "label": "snip", "insertTextFormat": 2, "insertText": "snip(${1:x}, $2)$0" },
        ]
    }))
    .unwrap();
    let entries = completions(Some(response));
    let labels: Vec<_> = entries.iter().map(|e| e.label.as_str()).collect();
    // sortText first ("1", "2"), then the label for those without.
    assert_eq!(labels, ["zeta", "alpha", "beta", "gamma", "snip"]);
    assert_eq!(entries[1].insert, "alpha()");
    assert_eq!(entries[1].detail.as_deref(), Some("fn alpha()"));
    assert_eq!(entries[2].insert, "beta_x");
    assert_eq!(entries[2].range, Some(range((0, 0), (0, 2))));
    assert_eq!(entries[2].filter, "bx");
    assert_eq!(entries[2].detail, None, "a blank detail is none");
    assert_eq!(
        entries[3].range,
        Some(range((0, 0), (0, 1))),
        "the insert range"
    );
    assert_eq!(entries[4].insert, "snip(x, )");
    assert_eq!(
        entries[0].insert, "zeta",
        "the label when nothing else is said"
    );
    // An array answer, and none.
    let array: CompletionResponse = serde_json::from_value(json!([{ "label": "x" }])).unwrap();
    assert_eq!(completions(Some(array)).len(), 1);
    assert!(completions(None).is_empty());
}

#[test]
fn completion_entries_match_what_was_typed() {
    assert!(matches("println", "pri"));
    assert!(matches("println", "PLn"));
    assert!(matches("println", "pn"), "in order, not together");
    assert!(matches("anything", ""));
    assert!(!matches("println", "rint"), "the first character first");
    assert!(!matches("println", "pnl"), "out of order");
    assert!(!matches("", "a"));
    assert!(matches("Ёлка", "ёл"));
}

#[test]
fn snippets_become_plain_text() {
    for (snippet, plain) in [
        ("foo($1)$0", "foo()"),
        ("${1:name}: ${2:Type}", "name: Type"),
        ("${1|one,two|} end", "one end"),
        ("${1:outer ${2:inner}}", "outer inner"),
        ("$TM_FILENAME x ${TM_LINE_NUMBER}", " x "),
        (r"cost \$5 \} \\", r"cost $5 } \"),
        (r"${1|a\,b,c|}", "a,b"),
        ("", ""),
        ("$", "$"),
        ("a $ b", "a $ b"),
        ("${", ""),
        ("${1:unclosed", "unclosed"),
        ("${1|a,b", "a"),
    ] {
        assert_eq!(snippet_text(snippet), plain, "{snippet:?}");
    }
}

#[test]
fn hover_contents_as_plain_text() {
    let markdown: HoverContents = serde_json::from_value(json!({
        "kind": "markdown",
        "value": "```rust\nfn main()\n```\n\n---\n\nRuns \\*first\\*.\n\n\n\nAnd last.  "
    }))
    .unwrap();
    assert_eq!(
        hover_text(&markdown),
        "fn main()\n\nRuns *first*.\n\nAnd last."
    );
    let plain: HoverContents =
        serde_json::from_value(json!({ "kind": "plaintext", "value": "  a *b*  \n" })).unwrap();
    assert_eq!(hover_text(&plain), "a *b*");
    let marked: HoverContents = serde_json::from_value(json!([
        { "language": "python", "value": "def f(): ..." },
        "",
        "Doc ~~~ here",
    ]))
    .unwrap();
    assert_eq!(hover_text(&marked), "def f(): ...\n\nDoc ~~~ here");
    let scalar: HoverContents = serde_json::from_value(json!("just text")).unwrap();
    assert_eq!(hover_text(&scalar), "just text");
    let empty: HoverContents =
        serde_json::from_value(json!({ "kind": "markdown", "value": "" })).unwrap();
    assert_eq!(hover_text(&empty), "");
}

#[test]
fn diagnostics_severity_and_message() {
    assert_eq!(
        Severity::of(Some(DiagnosticSeverity::ERROR)),
        Severity::Error
    );
    assert_eq!(
        Severity::of(Some(DiagnosticSeverity::WARNING)),
        Severity::Warning
    );
    assert_eq!(
        Severity::of(Some(DiagnosticSeverity::INFORMATION)),
        Severity::Information
    );
    assert_eq!(Severity::of(Some(DiagnosticSeverity::HINT)), Severity::Hint);
    assert_eq!(Severity::of(None), Severity::Error);
    assert!(Severity::Error < Severity::Hint, "the most serious first");
    for severity in [
        Severity::Error,
        Severity::Warning,
        Severity::Information,
        Severity::Hint,
    ] {
        assert!(!severity.label().is_empty());
    }
    let mut diagnostic = Diagnostic {
        message: " unused variable \n".into(),
        source: Some("rustc".into()),
        ..Diagnostic::default()
    };
    assert_eq!(diagnostic_message(&diagnostic), "rustc: unused variable");
    diagnostic.source = Some("  ".into());
    assert_eq!(diagnostic_message(&diagnostic), "unused variable");
    diagnostic.source = None;
    diagnostic.message = String::new();
    assert_eq!(diagnostic_message(&diagnostic), "");
}
