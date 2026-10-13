//! Tests of View > Hide Lines: hiding, the mark that shows them again, Show Hidden Lines.

use anyhow::Result;
use birchpad_commands::Invocation;
use gpui_kit::{Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext, point, px};

use crate::workspace::Workspace;
use crate::workspace::tests::{active_text, open_workspace, pane_views, secondary};

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

fn visible_lines(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<usize> {
    let view = pane_views(workspace, 0, cx)[0].clone();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let layout = view.layout.as_ref().expect("drawn");
        let mut lines: Vec<usize> = layout.rows.iter().map(|row| row.row.line).collect();
        lines.dedup();
        lines
    })
}

#[gpui_kit::test]
fn hiding_lines_and_showing_them_again(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("0\n1\n2\n3\n4");
    // Lines 1 and 2 (the selection ends at the start of line 3).
    cx.simulate_keystrokes(&secondary("home"));
    cx.simulate_keystrokes("down shift-down shift-down");
    cx.simulate_keystrokes("alt-h");
    assert_eq!(visible_lines(&workspace, cx), [0, 3, 4]);
    // The caret left the hidden lines, which are still there.
    let caret_line = |cx: &mut VisualTestContext| {
        workspace.read_with(cx, |workspace, cx| {
            let view = workspace.active_view(cx).unwrap().read(cx);
            birchpad_core::motion::line_of(view.text(cx), view.selection.primary().head)
        })
    };
    assert!(![1, 2].contains(&caret_line(cx)));
    assert_eq!(active_text(&workspace, cx), "0\n1\n2\n3\n4");

    // A click in the margin of the line above shows them again.
    let view = pane_views(&workspace, 0, cx)[0].clone();
    let (gutter, band) = view.read_with(cx, |view, _| {
        let layout = view.layout.as_ref().unwrap();
        (layout.margins.gutter, layout.hidden_marks[0].1)
    });
    let at = point(gutter.left() + px(2.), band.top() - px(4.));
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(visible_lines(&workspace, cx), [0, 1, 2, 3, 4]);

    // Hidden again, then Show Hidden Lines; the first line is never hidden.
    cx.simulate_keystrokes(&secondary("a"));
    cx.simulate_keystrokes("alt-h");
    assert_eq!(visible_lines(&workspace, cx), [0]);
    run(&workspace, Invocation::new("view.show-hidden-lines"), cx).unwrap();
    assert_eq!(visible_lines(&workspace, cx), [0, 1, 2, 3, 4]);
    cx.simulate_keystrokes(&secondary("home"));
    let error = run(&workspace, Invocation::new("view.hide-lines"), cx).unwrap_err();
    assert!(error.to_string().contains("first line"), "{error}");

    // A caret put in hidden lines (a search result, Go To) shows them.
    cx.simulate_keystrokes("down shift-down");
    cx.simulate_keystrokes("alt-h");
    assert_eq!(visible_lines(&workspace, cx), [0, 2, 3, 4]);
    view.update(cx, |view, cx| view.select_range(2..2, cx));
    assert_eq!(visible_lines(&workspace, cx), [0, 1, 2, 3, 4]);
}
