#![allow(
    clippy::single_range_in_vec_init,
    clippy::type_complexity,
    reason = "expected results written out"
)]

use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext};

use crate::workspace::open_workspace;
use birchpad::search_results::*;
use birchpad_core::search::LineHit;
use gpui_kit::AppContext as _;
use gpui_kit::MouseButton;

fn hit(line: usize, text: &str, matches: usize) -> LineHit {
    LineHit {
        line,
        text: text.to_owned(),
        highlights: Vec::new(),
        target: 0..1,
        matches,
    }
}

fn file(name: &str, lines: Vec<LineHit>) -> FileResults {
    let hits = lines.iter().map(|line| line.matches).sum();
    FileResults::new(
        ResultLocation::File(name.into()),
        name.to_owned(),
        lines,
        hits,
    )
}

/// The panel of a new window, focused, with a search for "foo" in a.txt (three hits on two
/// lines) and b.txt (one), of five documents searched.
fn panel_with_results(cx: &mut TestAppContext) -> (Entity<SearchResults>, &mut VisualTestContext) {
    let (workspace, cx) = open_workspace(cx);
    let results = workspace.read_with(cx, |workspace, _| workspace.search_results.clone());
    workspace.update_in(cx, |workspace, window, cx| {
        let run = SearchRun::new(
            "foo",
            vec![
                file("a.txt", vec![hit(0, "foo foo", 2), hit(2, "foo", 1)]),
                file("b.txt", vec![hit(1, "x foo", 1)]),
            ],
            5,
        );
        workspace.search_results.update(cx, |results, cx| {
            results.add(run, cx);
            window.focus(&results.focus_handle, cx);
        });
        cx.notify();
    });
    cx.run_until_parked();
    (results, cx)
}

fn rows(results: &Entity<SearchResults>, cx: &mut VisualTestContext) -> Vec<String> {
    results.read_with(cx, |results, _| results.rows_text())
}

fn selected(results: &Entity<SearchResults>, cx: &mut VisualTestContext) -> Option<usize> {
    results.read_with(cx, |results, _| results.selected)
}

#[gpui_kit::test]
fn delete_removes_the_selected_line_and_what_it_leaves_empty(cx: &mut TestAppContext) {
    let (results, cx) = panel_with_results(cx);
    // Nothing selected: Delete removes nothing.
    cx.simulate_keystrokes("delete");
    assert_eq!(rows(&results, cx).len(), 6);

    cx.simulate_keystrokes("down down down delete");
    assert_eq!(
        rows(&results, cx),
        [
            "Search \"foo\" (2 hits in 2 files of 5 searched)",
            "  a.txt (1)",
            "    Line 3: foo",
            "  b.txt (1)",
            "    Line 2: x foo",
        ],
        "the line's two hits no longer count"
    );
    assert_eq!(selected(&results, cx), Some(2), "the next line");
    // The last line of a document takes the document with it.
    cx.simulate_keystrokes("delete");
    assert_eq!(
        rows(&results, cx),
        [
            "Search \"foo\" (1 hit in 1 file of 5 searched)",
            "  b.txt (1)",
            "    Line 2: x foo",
        ]
    );
    assert_eq!(
        selected(&results, cx),
        Some(2),
        "the last row, past the end"
    );
    // The last document of a search takes the search with it.
    cx.simulate_keystrokes("delete");
    assert!(rows(&results, cx).is_empty());
    assert_eq!(selected(&results, cx), None);
    cx.simulate_keystrokes("delete down enter shift-delete");
    assert!(rows(&results, cx).is_empty());
}

#[gpui_kit::test]
fn delete_on_a_search_or_a_document_removes_everything_under_it(cx: &mut TestAppContext) {
    let (results, cx) = panel_with_results(cx);
    results.update(cx, |results, cx| {
        results.add(
            SearchRun::new("x", vec![file("c.txt", vec![hit(4, "x", 1)])], 1),
            cx,
        );
    });
    // The newer search is on top; its heading is row 0, the older one's row 3.
    cx.simulate_keystrokes("down down down down delete");
    assert_eq!(
        rows(&results, cx),
        [
            "Search \"x\" (1 hit in 1 file of 1 searched)",
            "  c.txt (1)",
            "    Line 5: x",
        ]
    );
    // Delete with a modifier is not Delete.
    cx.simulate_keystrokes("up shift-delete");
    assert_eq!(rows(&results, cx).len(), 3);
    cx.simulate_keystrokes("delete");
    assert!(
        rows(&results, cx).is_empty(),
        "the search went with its only document"
    );
    // F4 has nothing to go to.
    let stepped = results.update(cx, |results, cx| results.step(true, cx));
    assert!(!stepped);
}

#[gpui_kit::test]
fn the_right_click_menu_removes_copies_folds_and_clears(cx: &mut TestAppContext) {
    let (results, cx) = panel_with_results(cx);
    let right_click = |index: usize, cx: &mut VisualTestContext| {
        let selector: &'static str = format!("search-result-{index}").leak();
        let center = cx.debug_bounds(selector).expect("drawn").center();
        cx.simulate_mouse_down(center, MouseButton::Right, Modifiers::none());
        cx.simulate_mouse_up(center, MouseButton::Right, Modifiers::none());
        cx.run_until_parked();
    };
    let clipboard = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
    };

    // Remove, Copy Selected Line, Copy Selected Pathname, Open Selected Pathname, Open All,
    // Fold All, Unfold All, Wrap Long Lines, Purge for Every Search, Clear All; the arrow
    // keys skip the items that are disabled for the row.
    right_click(5, cx);
    assert_eq!(selected(&results, cx), Some(5), "the right click selects");
    cx.simulate_keystrokes("down down enter");
    assert_eq!(clipboard(cx).as_deref(), Some("x foo"));
    right_click(2, cx);
    cx.simulate_keystrokes("down down down enter");
    assert_eq!(clipboard(cx).as_deref(), Some("a.txt"));
    right_click(2, cx);
    cx.simulate_keystrokes("down enter");
    assert_eq!(rows(&results, cx)[2], "    Line 3: foo", "removed");

    // A search heading has neither a line nor a path to copy or open.
    right_click(0, cx);
    cx.simulate_keystrokes("down down down enter");
    assert_eq!(
        rows(&results, cx),
        ["Search \"foo\" (2 hits in 2 files of 5 searched)"],
        "folded"
    );
    right_click(0, cx);
    cx.simulate_keystrokes("down down down down enter");
    assert_eq!(rows(&results, cx).len(), 5, "unfolded");
    // A document has no line to copy.
    right_click(1, cx);
    cx.simulate_keystrokes("down down enter");
    assert_eq!(clipboard(cx).as_deref(), Some("a.txt"));
    // Escape closes the menu without doing anything.
    right_click(1, cx);
    cx.simulate_keystrokes("down escape");
    assert_eq!(rows(&results, cx).len(), 5);
    right_click(1, cx);
    cx.simulate_keystrokes("down down down down down down down down down enter");
    assert!(rows(&results, cx).is_empty(), "cleared");
}

#[gpui_kit::test]
fn a_search_heading_has_nothing_to_copy(cx: &mut TestAppContext) {
    let results = cx.new(SearchResults::new);
    results.update(cx, |results, cx| {
        results.add(
            SearchRun::new("foo", vec![file("a.txt", vec![hit(0, "foo", 1)])], 1),
            cx,
        );
        assert_eq!(results.line_text(0), None);
        assert_eq!(results.path_text(0), None);
        assert_eq!(results.line_text(1), None, "a document has no line");
        assert_eq!(results.path_text(1).as_deref(), Some("a.txt"));
        assert_eq!(results.line_text(2).as_deref(), Some("foo"));
        assert_eq!(results.path_text(2).as_deref(), Some("a.txt"));
        assert_eq!(results.line_text(3), None, "past the end");
        assert_eq!(results.path_text(3), None);
        // Removing a row that is not there changes nothing.
        results.remove(3, cx);
        assert_eq!(results.rows_text().len(), 3);
    });
}
