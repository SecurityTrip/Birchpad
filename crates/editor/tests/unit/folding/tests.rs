
use birchpad_commands::Invocation;
use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext, point};
use serde_json::json;

use super::*;
use crate::test_support::Workspace;
use crate::test_support::{block, document_start, open_workspace};

/// Folds: 0..4 (level 1), 1..3 (level 2), 5..7 (level 1).
const SAMPLE: &str = "fn a() {\n    if x {\n        y();\n    }\n}\nfn b() {\n    z();\n}\n";

fn view(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<EditorView> {
    workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap())
}

fn run(workspace: &Entity<Workspace>, invocation: Invocation, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&invocation, window, cx).unwrap();
    });
    cx.run_until_parked();
}

/// A Rust document with SAMPLE, parsed.
fn open_sample(cx: &mut TestAppContext) -> (Entity<Workspace>, &mut VisualTestContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input(SAMPLE);
    let rust = Invocation::with_args("language.set", json!({ "language": "rust" }));
    run(&workspace, rust, cx);
    cx.simulate_keystrokes(document_start());
    (workspace, cx)
}

fn hidden(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> Vec<std::ops::Range<usize>> {
    let view = view(workspace, cx);
    view.read_with(cx, |view, _| view.display.hidden().to_vec())
}

fn caret(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> (usize, usize) {
    let view = view(workspace, cx);
    view.read_with(cx, |view, cx| {
        let text = view.text(cx);
        let head = view.selection.primary().head;
        let line = line_of(text, head);
        (line, head - line_range(text, line).start)
    })
}

#[gpui_kit::test]
fn fold_levels_go_from_one_to_eight(cx: &mut TestAppContext) {
    let (workspace, cx) = open_sample(cx);
    let level = |command: &str, level: usize, cx: &mut VisualTestContext| {
        let invocation = Invocation::with_args(command, json!({ "level": level }));
        let result = workspace.update_in(cx, |workspace, window, cx| {
            workspace.dispatch(&invocation, window, cx)
        });
        cx.run_until_parked();
        result.map_err(|error| error.to_string())
    };

    // The lowest level, then the highest, which this sample does not reach.
    assert_eq!(level("view.fold-level", 1, cx), Ok(()));
    assert_eq!(hidden(&workspace, cx), [1..5, 6..8]);
    assert_eq!(level("view.unfold-level", 1, cx), Ok(()));
    assert!(hidden(&workspace, cx).is_empty());
    assert_eq!(level("view.fold-level", 8, cx), Ok(()));
    assert!(hidden(&workspace, cx).is_empty());

    // One past each end is refused and folds nothing.
    for past in [0, 9] {
        for command in ["view.fold-level", "view.unfold-level"] {
            assert_eq!(
                level(command, past, cx),
                Err(format!("fold levels go from 1 to 8, not {past}"))
            );
        }
    }
    assert!(hidden(&workspace, cx).is_empty());
}

#[gpui_kit::test]
fn fold_commands_hide_and_show_lines(cx: &mut TestAppContext) {
    let (workspace, cx) = open_sample(cx);
    cx.simulate_keystrokes("down down");
    run(&workspace, Invocation::new("view.fold-all"), cx);
    assert_eq!(hidden(&workspace, cx), [1..5, 6..8]);
    assert_eq!(
        caret(&workspace, cx),
        (0, 8),
        "the caret leaves the hidden lines"
    );

    run(&workspace, Invocation::new("view.unfold-all"), cx);
    assert!(hidden(&workspace, cx).is_empty());
    run(
        &workspace,
        Invocation::with_args("view.fold-level", json!({ "level": 2 })),
        cx,
    );
    assert_eq!(hidden(&workspace, cx), [2..4]);

    // The keys of Notepad++.
    cx.simulate_keystrokes("alt-shift-2");
    assert!(hidden(&workspace, cx).is_empty());
    cx.simulate_keystrokes("alt-0");
    assert_eq!(hidden(&workspace, cx), [1..5, 6..8]);
    cx.simulate_keystrokes("alt-shift-0");
    assert!(hidden(&workspace, cx).is_empty());

    // Collapse the innermost fold around the caret, then expand it from its header.
    cx.simulate_keystrokes("down down ctrl-alt-f");
    assert_eq!(hidden(&workspace, cx), [2..4]);
    assert_eq!(caret(&workspace, cx).0, 1);
    cx.simulate_keystrokes("ctrl-alt-shift-f");
    assert!(hidden(&workspace, cx).is_empty());
}

#[gpui_kit::test]
fn carets_step_over_collapsed_folds_and_reveal_them(cx: &mut TestAppContext) {
    let (workspace, cx) = open_sample(cx);
    cx.simulate_keystrokes("ctrl-alt-f end");
    assert_eq!(hidden(&workspace, cx), [1..5]);
    // Right at the end of the header goes past the fold, Left comes back.
    cx.simulate_keystrokes("right");
    assert_eq!(caret(&workspace, cx), (5, 0));
    cx.simulate_keystrokes("left");
    assert_eq!(caret(&workspace, cx), (0, 8));
    // Down goes to the next visible line.
    cx.simulate_keystrokes("down");
    assert_eq!(caret(&workspace, cx).0, 5);
    assert_eq!(hidden(&workspace, cx), [1..5], "still folded");

    // Going to a hidden line (Go To, a search result) expands the fold.
    let view = view(&workspace, cx);
    view.update(cx, |view, cx| {
        let pos = line_range(view.text(cx), 2).start;
        view.go_to(pos, cx);
    });
    cx.run_until_parked();
    assert!(hidden(&workspace, cx).is_empty());
    assert_eq!(caret(&workspace, cx), (2, 0));
}

#[gpui_kit::test]
fn the_folding_margin_toggles_and_folds_follow_edits(cx: &mut TestAppContext) {
    let (workspace, cx) = open_sample(cx);
    let view = view(&workspace, cx);
    // The fold box of `fn b` (row 5).
    let click = view.read_with(cx, |view, _| {
        let layout = view.layout.as_ref().expect("a frame was drawn");
        let margin = layout.margins.folding.expect("folding margin");
        let row = layout.rows.iter().find(|row| row.row.line == 5).unwrap();
        point(
            margin.left() + margin.size.width / 2.,
            row.y + layout.metrics.line_height / 2.,
        )
    });
    cx.simulate_click(click, Modifiers::none());
    assert_eq!(hidden(&workspace, cx), [6..8]);

    // A line typed above moves the collapsed fold down with its text, and it stays
    // collapsed after the reparse.
    cx.simulate_keystrokes(document_start());
    cx.simulate_input("// top\n");
    cx.run_until_parked();
    assert_eq!(hidden(&workspace, cx), [7..9]);

    cx.simulate_click(click, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        hidden(&workspace, cx),
        [7..9],
        "row 5 is no longer a fold point"
    );
}

#[gpui_kit::test]
fn shifted_folds_equal_recomputed_ones(cx: &mut TestAppContext) {
    let (workspace, cx) = open_sample(cx);
    // Inside `if x { ... }`: every fold is before, around or after the edited line.
    cx.simulate_keystrokes("down down end");
    cx.simulate_input(
        "
        w();
        v();",
    );
    let view = view(&workspace, cx);
    view.update(cx, |view, cx| {
        let shifted = view.fold_cache.as_ref().unwrap();
        let buffer = view.buffer.read(cx);
        let (ranges, version, _) = buffer.folds();
        assert_eq!(shifted.version, version, "shifted, not recomputed");
        let recomputed = folds_from_ranges(buffer.doc().text(), ranges);
        assert_eq!(shifted.folds, recomputed);
    });
}

#[gpui_kit::test]
fn rectangles_and_multi_selections_stay_out_of_collapsed_folds(cx: &mut TestAppContext) {
    let (workspace, cx) = open_sample(cx);
    // Collapse `fn a`, then extend a rectangle down from its header: it skips the
    // hidden lines and the fold stays collapsed.
    cx.simulate_keystrokes(&format!("ctrl-alt-f {}", block("down")));
    assert_eq!(hidden(&workspace, cx), [1..5]);
    let view = view(&workspace, cx);
    let lines: Vec<usize> = view.read_with(cx, |view, cx| {
        let text = view.text(cx);
        view.selection
            .iter()
            .map(|range| line_of(text, range.head))
            .collect()
    });
    assert_eq!(lines, [0, 5]);
    cx.simulate_input("  ");
    cx.run_until_parked();
    let text = view.read_with(cx, |view, cx| view.text(cx).to_string());
    assert!(text.starts_with("  fn a() {\n    if x {"), "{text}");
    assert!(text.contains("\n  fn b()"), "{text}");
    assert_eq!(hidden(&workspace, cx), [1..5], "hidden lines untouched");

    // Multi-select All finds matches inside the fold: the fold opens. (The indentation
    // of `z();`, which the hidden lines have too.)
    cx.simulate_keystrokes("escape down home shift-home");
    run(&workspace, Invocation::new("edit.multi-select-all"), cx);
    cx.run_until_parked();
    let selected = view.read_with(cx, |view, _| view.selection.ranges().len());
    assert!(selected > 1);
    assert!(hidden(&workspace, cx).is_empty());
}
