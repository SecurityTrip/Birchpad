//! Tests of split view: moving and cloning documents to the other view, views of one buffer
//! following each other's edits, closing a clone, focus, dropped tabs and synchronized
//! scrolling.

use birchpad_commands::Invocation;
use birchpad_config::SplitOrientation;
use birchpad_core::{Document, Range, Rope, Selection};
use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext, px};

use crate::app_state::AppState;
use crate::editor::EditorView;
use crate::workspace::Workspace;
use crate::workspace::tests::{
    active_text, document_start, drop_tab, open_workspace, pane_views, panes, secondary,
};

fn run(workspace: &Entity<Workspace>, command: &str, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace
            .dispatch(&Invocation::new(command), window, cx)
            .unwrap();
    });
    cx.run_until_parked();
}

fn active_view(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<EditorView> {
    workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap())
}

fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

#[gpui_kit::test]
fn move_and_clone_documents_to_the_other_view(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_keystrokes(&secondary("n"));
    assert_eq!(
        panes(&workspace, cx),
        ([names(&["new 1", "new 2"]), vec![]], 0)
    );

    // The second view appears with the moved document and becomes the active one.
    run(&workspace, "view.move-to-other-view", cx);
    assert_eq!(
        panes(&workspace, cx),
        ([names(&["new 1"]), names(&["new 2"])], 1)
    );
    cx.simulate_input("two");

    // A clone is another view of the same buffer.
    run(&workspace, "view.clone-to-other-view", cx);
    assert_eq!(
        panes(&workspace, cx),
        ([names(&["new 1", "new 2"]), names(&["new 2"])], 0)
    );
    assert_eq!(active_text(&workspace, cx), "two");
    let [main, second] = [0, 1].map(|pane| pane_views(&workspace, pane, cx));
    let same_buffer = main[1].read_with(cx, |clone, cx| clone.buffer == second[0].read(cx).buffer);
    assert!(same_buffer);

    // Cloning again only goes to the existing view; moving there closes this tab.
    run(&workspace, "view.clone-to-other-view", cx);
    assert_eq!(
        panes(&workspace, cx),
        ([names(&["new 1", "new 2"]), names(&["new 2"])], 1)
    );
    run(&workspace, "view.focus-other-view", cx);
    run(&workspace, "view.move-to-other-view", cx);
    assert_eq!(
        panes(&workspace, cx),
        ([names(&["new 1"]), names(&["new 2"])], 1)
    );

    // Moving the last tab of a view hides it.
    run(&workspace, "view.move-to-other-view", cx);
    assert_eq!(
        panes(&workspace, cx),
        ([names(&["new 1", "new 2"]), vec![]], 0)
    );
}

#[gpui_kit::test]
fn views_of_one_buffer_follow_each_others_edits(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("a {\n    b\n}\nc");
    cx.simulate_keystrokes(document_start());
    run(&workspace, "view.clone-to-other-view", cx);
    let clone = active_view(&workspace, cx);
    // In the clone: a caret on "c", a bookmark on "}", the block collapsed.
    clone.update(cx, |clone, cx| {
        clone.selection = Selection::single(Range::point(12));
        cx.notify();
    });
    cx.simulate_keystrokes("up ctrl-f2");
    run(&workspace, "view.fold-all", cx);
    let clone_state = |cx: &mut VisualTestContext| {
        clone.read_with(cx, |clone, cx| {
            let buffer = clone.buffer.read(cx);
            (
                clone.selection.primary(),
                buffer.marks().bookmarks.lines(buffer.doc().text()),
                clone.collapsed_lines(cx),
            )
        })
    };
    assert_eq!(clone_state(cx), (Range::point(10), vec![2], vec![0]));

    // Typing two lines in the original moves all of them down.
    run(&workspace, "view.focus-other-view", cx);
    cx.simulate_input("x\ny\n");
    assert_eq!(clone_state(cx), (Range::point(14), vec![4], vec![2]));
    let original = active_view(&workspace, cx);
    let original_folds = original.read_with(cx, |view, cx| view.collapsed_lines(cx));
    assert!(original_folds.is_empty(), "folding is each view's own");
}

#[gpui_kit::test]
fn closing_a_clone_keeps_the_document(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("unsaved");
    run(&workspace, "view.clone-to-other-view", cx);
    cx.simulate_keystrokes(&secondary("w"));
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt(), "the document is still open");
    assert_eq!(panes(&workspace, cx), ([names(&["new 1"]), vec![]], 0));
    assert_eq!(active_text(&workspace, cx), "unsaved");

    // Closing the last view asks.
    cx.simulate_keystrokes(&secondary("w"));
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Don't Save");
}

#[gpui_kit::test]
fn focus_moves_between_views(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("main");
    cx.simulate_keystrokes(&secondary("n"));
    cx.simulate_input("second");
    run(&workspace, "view.move-to-other-view", cx);
    assert_eq!(panes(&workspace, cx).1, 1);

    // F8 goes to the other view; typing goes there.
    cx.simulate_keystrokes("f8");
    assert_eq!(panes(&workspace, cx).1, 0);
    cx.simulate_input("!");
    assert_eq!(active_text(&workspace, cx), "main!");

    // A click in the other view makes its pane the active one.
    let second = pane_views(&workspace, 1, cx)[0].clone();
    let bounds = second.read_with(cx, |view, _| view.bounds().expect("drawn"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    assert_eq!(panes(&workspace, cx).1, 1);
    assert_eq!(active_text(&workspace, cx), "second");

    // Ctrl+Tab stays in the view.
    cx.simulate_keystrokes(&secondary("n"));
    cx.simulate_keystrokes("ctrl-tab");
    assert_eq!(
        panes(&workspace, cx),
        ([names(&["new 1"]), names(&["new 2", "new 3"])], 1)
    );
    assert_eq!(active_text(&workspace, cx), "second");
}

#[gpui_kit::test]
fn dropped_tabs_reorder_or_change_views(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_keystrokes(&secondary("n"));
    cx.simulate_keystrokes(&secondary("n"));
    let views = pane_views(&workspace, 0, cx);

    // On a tab of the same pane: the tab goes there.
    drop_tab(&workspace, 0, views[2].clone(), 0, cx);
    assert_eq!(
        panes(&workspace, cx).0,
        [names(&["new 3", "new 1", "new 2"]), vec![]]
    );
    drop_tab(&workspace, 0, views[2].clone(), 3, cx);
    assert_eq!(
        panes(&workspace, cx).0,
        [names(&["new 1", "new 2", "new 3"]), vec![]]
    );

    // On the other pane: the tab moves there.
    run(&workspace, "view.move-to-other-view", cx);
    drop_tab(&workspace, 1, views[0].clone(), 0, cx);
    assert_eq!(
        panes(&workspace, cx),
        ([names(&["new 2"]), names(&["new 1", "new 3"])], 1)
    );
}

#[gpui_kit::test]
fn synchronized_scrolling_keeps_the_offset(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let text = vec!["x".repeat(2000); 300].join("\n");
    workspace.update_in(cx, |workspace, window, cx| {
        let doc = Document::from_text(Rope::from_str(&text));
        workspace.open_document(doc, window, cx);
    });
    cx.run_until_parked();
    run(&workspace, "view.clone-to-other-view", cx);
    // The document went after "new 1"; its clone is alone in the second view.
    let main = pane_views(&workspace, 0, cx)[1].clone();
    let second = pane_views(&workspace, 1, cx)[0].clone();
    let position = |view: &Entity<EditorView>, cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| view.scroll_position())
    };

    // Not synchronized: the main view stays.
    cx.simulate_keystrokes("pagedown");
    assert_eq!(position(&main, cx), (0., px(0.)));
    let scrolled = position(&second, cx);
    assert!(scrolled.0 > 0.);

    // Synchronized, the main view scrolls by as many rows and pixels and keeps its offset.
    run(&workspace, "view.sync-vertical-scroll", cx);
    run(&workspace, "view.sync-horizontal-scroll", cx);
    cx.simulate_keystrokes("pagedown end");
    cx.run_until_parked();
    let now = position(&second, cx);
    let main_now = position(&main, cx);
    assert_eq!(main_now.0, now.0 - scrolled.0, "as many rows");
    // As many pixels, unless the main view cannot scroll that far right: it keeps a smaller
    // margin past the end of the line than the caret asks for.
    assert!(now.1 > px(0.) && main_now.1 > now.1 - px(100.));

    // Off again.
    run(&workspace, "view.sync-vertical-scroll", cx);
    run(&workspace, "view.sync-horizontal-scroll", cx);
    let main_before = position(&main, cx);
    cx.simulate_keystrokes(document_start());
    cx.run_until_parked();
    assert_eq!(position(&main, cx), main_before);
}

#[gpui_kit::test]
fn rotating_the_split_is_remembered(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    run(&workspace, "view.clone-to-other-view", cx);
    let split = |cx: &mut VisualTestContext| cx.update(|_, cx| AppState::global(cx).state.split);
    assert_eq!(split(cx), SplitOrientation::SideBySide);
    run(&workspace, "view.rotate-split", cx);
    assert_eq!(split(cx), SplitOrientation::Stacked);
    // Both views are still drawn, one above the other.
    let tops = [0, 1].map(|pane| {
        let view = pane_views(&workspace, pane, cx)[0].clone();
        view.read_with(cx, |view, _| view.bounds().map(|bounds| bounds.top()))
    });
    let [Some(top), Some(bottom)] = tops else {
        panic!("both views drawn: {tops:?}");
    };
    assert!(bottom > top);
    run(&workspace, "view.rotate-split", cx);
    assert_eq!(split(cx), SplitOrientation::SideBySide);
}
