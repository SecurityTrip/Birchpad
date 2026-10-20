use gpui_kit::{TestAppContext, VisualTestContext};
use serde_json::json;

use crate::workspace::{open_workspace, panes};
use anyhow::Result;
use birchpad::compare::*;
use birchpad::workspace::Workspace;
use birchpad_commands::Invocation;
use birchpad_core::Rope;
use birchpad_tools::compare::Change;
use birchpad_tools::compare::Options;
use gpui_kit::Entity;

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

/// The compared lines of each view's active document, as (first line, change).
fn marked(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> [Vec<(usize, Change)>; 2] {
    workspace.read_with(cx, |workspace, cx| {
        [0, 1].map(|pane| {
            let Some(view) = workspace.panes()[pane].read(cx).active_item() else {
                return Vec::new();
            };
            let buffer = view.read(cx).buffer.read(cx);
            let text = buffer.doc().text();
            buffer
                .marks()
                .compare
                .iter()
                .map(|(range, change)| (birchpad_core::motion::line_of(text, range.start), *change))
                .collect()
        })
    })
}

fn caret(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> usize {
    workspace.read_with(cx, |workspace, cx| {
        workspace
            .active_view(cx)
            .unwrap()
            .read(cx)
            .selection
            .primary()
            .head
    })
}

#[gpui_kit::test]
fn compares_the_active_document_with_the_tab_before_it(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "a\nb\nc\nd\n", cx);
    run(&workspace, Invocation::new("file.new"), cx).unwrap();
    type_text(&workspace, "a\nB\nc\nd\ne\n", cx);
    run(&workspace, Invocation::new("compare.compare"), cx).unwrap();
    // Side by side, the new document in the other view.
    assert_eq!(
        panes(&workspace, cx).0,
        [vec!["new 1".to_owned()], vec!["new 2".to_owned()]]
    );
    assert_eq!(
        marked(&workspace, cx),
        [
            vec![(1, Change::Changed)],
            vec![(1, Change::Changed), (4, Change::Added)]
        ]
    );
    // The caret is at the first difference; the next ones follow, wrapping around.
    assert_eq!(caret(&workspace, cx), 2);
    run(&workspace, Invocation::new("compare.next"), cx).unwrap();
    assert_eq!(caret(&workspace, cx), 8);
    run(&workspace, Invocation::new("compare.next"), cx).unwrap();
    assert_eq!(caret(&workspace, cx), 2);
    run(&workspace, Invocation::new("compare.previous"), cx).unwrap();
    assert_eq!(caret(&workspace, cx), 8);

    run(&workspace, Invocation::new("compare.clear"), cx).unwrap();
    assert_eq!(marked(&workspace, cx), [vec![], vec![]]);
    assert!(run(&workspace, Invocation::new("compare.next"), cx).is_err());
}

#[gpui_kit::test]
fn the_old_document_shows_what_it_loses(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "keep\ngone\nkeep too", cx);
    run(&workspace, Invocation::new("file.new"), cx).unwrap();
    type_text(&workspace, "keep\nkeep too", cx);
    run(&workspace, Invocation::new("compare.compare"), cx).unwrap();
    assert_eq!(marked(&workspace, cx), [vec![(1, Change::Removed)], vec![]]);
    // Comparing again from the other view: the views are already side by side.
    run(&workspace, Invocation::new("view.focus-other-view"), cx).unwrap();
    run(&workspace, Invocation::new("compare.compare"), cx).unwrap();
    assert_eq!(marked(&workspace, cx), [vec![(1, Change::Added)], vec![]]);
}

#[gpui_kit::test]
fn can_ignore_spaces_and_case(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "if (a)\n", cx);
    run(&workspace, Invocation::new("file.new"), cx).unwrap();
    type_text(&workspace, "IF ( a )\n", cx);
    run(&workspace, Invocation::new("compare.ignore-spaces"), cx).unwrap();
    run(&workspace, Invocation::new("compare.compare"), cx).unwrap();
    assert_eq!(marked(&workspace, cx)[1], [(0, Change::Changed)]);
    run(&workspace, Invocation::new("compare.ignore-case"), cx).unwrap();
    run(&workspace, Invocation::new("compare.compare"), cx).unwrap();
    assert_eq!(marked(&workspace, cx), [vec![], vec![]]);
    let current = cx.update(|_, cx| options(cx));
    assert_eq!(
        is_checked(&Invocation::new("compare.ignore-case"), current),
        Some(true)
    );
    assert_eq!(
        is_checked(&Invocation::new("compare.ignore-spaces"), current),
        Some(true)
    );
    assert_eq!(is_checked(&Invocation::new("file.new"), current), None);
}

#[gpui_kit::test]
fn needs_two_documents(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let error = run(&workspace, Invocation::new("compare.compare"), cx).unwrap_err();
    assert!(error.to_string().contains("second document"), "{error}");
    // The same document in both views is no comparison.
    run(&workspace, Invocation::new("view.clone-to-other-view"), cx).unwrap();
    let error = run(&workspace, Invocation::new("compare.compare"), cx).unwrap_err();
    assert!(error.to_string().contains("same as itself"), "{error}");
}

#[test]
fn marks_whole_lines_on_each_side() {
    let old = Rope::from("a\nb\nc");
    let new = Rope::from("a\nc\nd");
    let differences = birchpad_tools::compare::compare("a\nb\nc", "a\nc\nd", Options::default());
    let [old_marks, new_marks] = marks(&old, &new, &differences);
    assert_eq!(old_marks, [(2..4, Change::Removed)]);
    assert_eq!(new_marks, [(4..5, Change::Added)]);
    assert_eq!(byte_range(&old, &(0..0)), 0..0);
    assert_eq!(byte_range(&old, &(0..3)), 0..5);
}
