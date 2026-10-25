use birchpad_commands::Invocation;
use birchpad_core::{Document, Rope};
use birchpad_editor::syntax_selection::expanded;
use birchpad_editor::*;
use birchpad_ui::AppState;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use crate::test_support::{Workspace, open_workspace};

const SAMPLE: &str = "fn a() {\n    g(\"hi\", 1);\n}\n";

fn view(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<EditorView> {
    workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap())
}

fn run(workspace: &Entity<Workspace>, id: &str, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace
            .dispatch(&Invocation::new(id), window, cx)
            .unwrap();
    });
    cx.run_until_parked();
}

/// A document with `text` in `language`, parsed.
fn open<'a>(
    text: &str,
    language: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<Workspace>, &'a mut VisualTestContext) {
    let (workspace, cx) = open_workspace(cx);
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_document(Document::from_text(Rope::from_str(text)), window, cx);
        let set = Invocation::with_args("language.set", json!({ "language": language }));
        workspace.dispatch(&set, window, cx).unwrap();
    });
    cx.run_until_parked();
    (workspace, cx)
}

fn selected(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    let view = view(workspace, cx);
    view.read_with(cx, |view, cx| {
        let range = view.selection.primary();
        view.text(cx).slice(range.from()..range.to()).to_string()
    })
}

fn put_caret(workspace: &Entity<Workspace>, pos: usize, cx: &mut VisualTestContext) {
    let view = view(workspace, cx);
    view.update(cx, |view, cx| view.go_to(pos, cx));
    cx.run_until_parked();
}

#[gpui_kit::test]
fn expand_grows_by_the_syntax_and_shrink_goes_back(cx: &mut TestAppContext) {
    let (workspace, cx) = open(SAMPLE, "rust", cx);
    put_caret(&workspace, SAMPLE.find("hi").unwrap() + 1, cx);
    let mut grown = Vec::new();
    for _ in 0..6 {
        run(&workspace, "edit.expand-selection", cx);
        grown.push(selected(&workspace, cx));
    }
    assert_eq!(grown[0], "hi");
    assert_eq!(grown[1], "\"hi\"");
    assert_eq!(grown[2], "\"hi\", 1");
    assert_eq!(grown[3], "(\"hi\", 1)");
    assert!(grown.iter().any(|text| text == "g(\"hi\", 1)"), "{grown:?}");
    // Each holds the one before.
    for pair in grown.windows(2) {
        assert!(pair[1].contains(pair[0].as_str()), "{pair:?}");
    }
    run(&workspace, "edit.shrink-selection", cx);
    assert_eq!(selected(&workspace, cx), grown[4]);
    for _ in 0..10 {
        run(&workspace, "edit.shrink-selection", cx);
    }
    // Back to the caret, and no further.
    assert_eq!(selected(&workspace, cx), "");
}

#[gpui_kit::test]
fn shrink_does_nothing_once_the_selection_moved(cx: &mut TestAppContext) {
    let (workspace, cx) = open(SAMPLE, "rust", cx);
    put_caret(&workspace, SAMPLE.find("hi").unwrap(), cx);
    run(&workspace, "edit.expand-selection", cx);
    run(&workspace, "edit.expand-selection", cx);
    put_caret(&workspace, 0, cx);
    run(&workspace, "edit.shrink-selection", cx);
    assert_eq!(selected(&workspace, cx), "");
    // Without an expansion at all.
    run(&workspace, "edit.shrink-selection", cx);
    assert_eq!(selected(&workspace, cx), "");
}

#[gpui_kit::test]
fn plain_text_grows_to_the_word_the_line_and_all(cx: &mut TestAppContext) {
    let text = "hello wide мир\nnext";
    let (workspace, cx) = open(text, "text", cx);
    put_caret(&workspace, text.find("мир").unwrap() + 2, cx);
    run(&workspace, "edit.expand-selection", cx);
    assert_eq!(selected(&workspace, cx), "мир");
    run(&workspace, "edit.expand-selection", cx);
    assert_eq!(selected(&workspace, cx), "hello wide мир");
    run(&workspace, "edit.expand-selection", cx);
    assert_eq!(selected(&workspace, cx), text);
    // All of it: nothing more.
    run(&workspace, "edit.expand-selection", cx);
    assert_eq!(selected(&workspace, cx), text);
    run(&workspace, "edit.shrink-selection", cx);
    assert_eq!(selected(&workspace, cx), "hello wide мир");
}

#[test]
fn expanded_without_syntax_reaches_the_whole_text() {
    let text = Rope::from_str("ab cd\nef");
    assert_eq!(expanded(None, &text, 1..1), 0..2);
    assert_eq!(expanded(None, &text, 0..2), 0..5);
    assert_eq!(expanded(None, &text, 0..5), 0..8);
    assert_eq!(expanded(None, &text, 0..8), 0..8);
    // Across lines: the whole text.
    assert_eq!(expanded(None, &text, 4..7), 0..8);
    // A caret at the end grows to the word before it.
    assert_eq!(expanded(None, &text, 8..8), 6..8);
    let empty = Rope::from_str("");
    assert_eq!(expanded(None, &empty, 0..0), 0..0);
}

#[gpui_kit::test]
fn sticky_scroll_keeps_the_headers_around_the_top(cx: &mut TestAppContext) {
    let mut text = String::from("fn outer() {\n    if x {\n");
    for line in 0..200 {
        text.push_str(&format!("        y{line}();\n"));
    }
    text.push_str("    }\n}\n");
    let (workspace, cx) = open(&text, "rust", cx);
    let view = view(&workspace, cx);
    let sticky = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.layout
                .as_ref()
                .map(|layout| layout.sticky.iter().map(|row| row.line).collect::<Vec<_>>())
                .unwrap_or_default()
        })
    };
    // The caret there too: a change of settings scrolls back to it.
    let line_50 = text
        .lines()
        .take(50)
        .map(|line| line.len() + 1)
        .sum::<usize>();
    view.update(cx, |view, cx| {
        view.go_to(line_50, cx);
        view.scroll_to_line(50, cx);
    });
    cx.run_until_parked();
    assert!(sticky(cx).is_empty(), "off by default");
    cx.update(|_, cx| {
        let editor = &mut cx.global_mut::<AppState>().settings.editor;
        editor.sticky_scroll = true;
        editor.sticky_scroll_lines = 1;
    });
    view.update(cx, |view, cx| view.settings_changed(cx));
    cx.run_until_parked();
    assert_eq!(sticky(cx), [0], "one line at most");
    cx.update(|_, cx| {
        cx.global_mut::<AppState>()
            .settings
            .editor
            .sticky_scroll_lines = 5
    });
    view.update(cx, |view, cx| view.settings_changed(cx));
    cx.run_until_parked();
    assert_eq!(sticky(cx), [0, 1]);
    // At the top of the document: none.
    view.update(cx, |view, cx| view.scroll_to_line(0, cx));
    cx.run_until_parked();
    assert!(sticky(cx).is_empty());
}
