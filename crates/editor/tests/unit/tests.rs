use gpui_kit::{Modifiers, TestAppContext, VisualTestContext};

use super::*;
use crate::test_support::Workspace;
use crate::test_support::{active_text, document_start, open_workspace};

/// Ctrl (Option on macOS) with an arrow or Backspace/Delete: by words.
fn word(key: &str) -> String {
    let modifier = if cfg!(target_os = "macos") {
        "alt"
    } else {
        "ctrl"
    };
    format!("{modifier}-{key}")
}

fn primary(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Range {
    workspace.read_with(cx, |workspace, cx| {
        workspace
            .active_view(cx)
            .unwrap()
            .read(cx)
            .selection
            .primary()
    })
}

fn set_text(text: &str, cx: &mut VisualTestContext) {
    cx.simulate_input(text);
    cx.simulate_keystrokes(document_start());
}

#[gpui_kit::test]
fn word_movement_and_deletion(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    set_text("let value = other_value;", cx);
    cx.simulate_keystrokes(&word("right"));
    assert_eq!(primary(&workspace, cx), Range::point(4));
    cx.simulate_keystrokes(&format!("{} {}", word("right"), word("right")));
    assert_eq!(primary(&workspace, cx), Range::point(12));
    cx.simulate_keystrokes(&format!("shift-{}", word("right")));
    assert_eq!(primary(&workspace, cx), Range::new(12, 23));
    cx.simulate_keystrokes(&word("left"));
    assert_eq!(primary(&workspace, cx), Range::point(12));

    cx.simulate_keystrokes(&word("delete"));
    assert_eq!(active_text(&workspace, cx), "let value = ;");
    cx.simulate_keystrokes(&word("backspace"));
    assert_eq!(active_text(&workspace, cx), "let value ;");
}

#[gpui_kit::test]
fn smart_home_toggles_between_indent_and_line_start(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    set_text("    code", cx);
    cx.simulate_keystrokes("end home");
    assert_eq!(primary(&workspace, cx), Range::point(4));
    cx.simulate_keystrokes("home");
    assert_eq!(primary(&workspace, cx), Range::point(0));
    cx.simulate_keystrokes("home");
    assert_eq!(primary(&workspace, cx), Range::point(4));
    cx.simulate_keystrokes("shift-end");
    assert_eq!(primary(&workspace, cx), Range::new(4, 8));
}

#[gpui_kit::test]
fn insert_key_toggles_overwrite(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    set_text("abcd\nef", cx);
    cx.simulate_keystrokes("insert");
    cx.simulate_input("XY");
    assert_eq!(active_text(&workspace, cx), "XYcd\nef");
    // Overwriting stops at the end of the line.
    cx.simulate_input("123");
    assert_eq!(active_text(&workspace, cx), "XY123\nef");
    cx.simulate_keystrokes("insert");
    cx.simulate_input("!");
    assert_eq!(active_text(&workspace, cx), "XY123!\nef");
}

#[gpui_kit::test]
fn tab_inserts_spaces_to_the_next_stop_when_configured(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().settings.editor.insert_spaces = true;
    });
    cx.simulate_input("ab");
    cx.simulate_keystrokes("tab");
    assert_eq!(active_text(&workspace, cx), "ab  ");
    cx.simulate_keystrokes("tab");
    assert_eq!(active_text(&workspace, cx), "ab      ");
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().settings.editor.insert_spaces = false;
    });
    cx.simulate_keystrokes("tab");
    assert_eq!(active_text(&workspace, cx), "ab      \t");
}

#[gpui_kit::test]
fn vertical_movement_keeps_the_column(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    set_text("abcdef\nx\nabcdef", cx);
    cx.simulate_keystrokes("right right right right down");
    assert_eq!(
        primary(&workspace, cx),
        Range::point(8),
        "end of the short line"
    );
    cx.simulate_keystrokes("down");
    assert_eq!(primary(&workspace, cx), Range::point(13), "column 4 again");
    cx.simulate_keystrokes("shift-up shift-up");
    assert_eq!(primary(&workspace, cx), Range::new(13, 4));
}

fn bookmarks(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<usize> {
    workspace.read_with(cx, |workspace, cx| {
        let buffer = workspace.active_view(cx).unwrap().read(cx).buffer.read(cx);
        buffer.marks().bookmarks.lines(buffer.doc().text())
    })
}

/// Window points in the middle of `row` (a visible row index) in the symbol margin and in
/// the line number margin, from the last frame's layout.
fn margin_points(
    workspace: &Entity<Workspace>,
    row: usize,
    cx: &mut VisualTestContext,
) -> (Point<Pixels>, Point<Pixels>) {
    workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        let layout = view.read(cx).layout.as_ref().expect("a frame was drawn");
        let y = layout.rows[row].y + layout.metrics.line_height / 2.;
        let center = |bounds: Bounds<Pixels>| point(bounds.left() + bounds.size.width / 2., y);
        (
            center(layout.margins.symbols.expect("symbol margin")),
            center(layout.margins.line_numbers.expect("line number margin")),
        )
    })
}

#[gpui_kit::test]
fn margin_clicks_toggle_bookmarks_and_select_lines(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input(
        "one
two
three",
    );
    let (symbol, _) = margin_points(&workspace, 1, cx);
    cx.simulate_click(symbol, Modifiers::none());
    assert_eq!(bookmarks(&workspace, cx), [1]);

    // A line inserted above moves the bookmark down with its line.
    cx.simulate_keystrokes(document_start());
    cx.simulate_input(
        "zero
",
    );
    assert_eq!(bookmarks(&workspace, cx), [2]);
    let (symbol, number) = margin_points(&workspace, 2, cx);
    cx.simulate_click(symbol, Modifiers::none());
    assert!(bookmarks(&workspace, cx).is_empty());

    // A click on a line number selects the line with its line break.
    cx.simulate_click(number, Modifiers::none());
    assert_eq!(primary(&workspace, cx), Range::new(9, 13));
}

fn scroll_left(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Pixels {
    workspace.read_with(cx, |workspace, cx| {
        workspace.active_view(cx).unwrap().read(cx).scroll_left
    })
}

/// Window points on the first row: at its column zero, just inside the left edge of the
/// text area, and just past its left and right edges, from the last frame's layout.
fn edge_points(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> (Point<Pixels>, Point<Pixels>, Point<Pixels>, Point<Pixels>) {
    workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        let layout = view.read(cx).layout.as_ref().expect("a frame was drawn");
        let y = layout.rows[0].y + layout.metrics.line_height / 2.;
        (
            point(layout.column_zero, y),
            point(layout.text_bounds.left() + px(8.), y),
            point(layout.text_bounds.left() - px(4.), y),
            point(layout.text_bounds.right() + px(4.), y),
        )
    })
}

#[gpui_kit::test]
fn dragging_past_the_right_edge_scrolls_and_extends_the_selection(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let line = "x".repeat(2000);
    set_text(&line, cx);
    let (start, _, _, right) = edge_points(&workspace, cx);
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(right, MouseButton::Left, Modifiers::none());
    let first = primary(&workspace, cx);
    assert!(scroll_left(&workspace, cx) > px(0.), "scrolled right");
    for _ in 0..20 {
        cx.simulate_mouse_move(right, MouseButton::Left, Modifiers::none());
    }
    let last = primary(&workspace, cx);
    assert_eq!(last.anchor, 0);
    assert!(
        last.head > first.head,
        "the selection went on past the first visible part: {first:?} -> {last:?}"
    );
    cx.simulate_mouse_up(right, MouseButton::Left, Modifiers::none());
}

#[gpui_kit::test]
fn dragging_past_the_left_edge_scrolls_back_to_the_line_start(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let line = "x".repeat(2000);
    set_text(&line, cx);
    cx.simulate_keystrokes("end");
    assert!(scroll_left(&workspace, cx) > px(0.));
    let (_, inside, left, _) = edge_points(&workspace, cx);
    cx.simulate_mouse_down(inside, MouseButton::Left, Modifiers::none());
    for _ in 0..1000 {
        cx.simulate_mouse_move(left, MouseButton::Left, Modifiers::none());
        if scroll_left(&workspace, cx) == px(0.) {
            break;
        }
    }
    cx.simulate_mouse_move(left, MouseButton::Left, Modifiers::none());
    // At the line start it stops there.
    assert_eq!(scroll_left(&workspace, cx), px(0.));
    assert_eq!(primary(&workspace, cx).head, 0);
}

#[gpui_kit::test]
fn dragging_inside_the_text_area_does_not_scroll_sideways(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let line = "x".repeat(2000);
    set_text(&line, cx);
    let (start, _, _, right) = edge_points(&workspace, cx);
    let inside = point(right.x - px(40.), right.y);
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    for _ in 0..5 {
        cx.simulate_mouse_move(inside, MouseButton::Left, Modifiers::none());
    }
    assert_eq!(scroll_left(&workspace, cx), px(0.));
    assert!(primary(&workspace, cx).head > 0);
}

#[gpui_kit::test]
#[ignore = "a timing; run in release: cargo test --release -- --ignored --nocapture"]
fn typing_into_a_ten_megabyte_line(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let line = "0123456789 abcdef\t".repeat(10 * 1024 * 1024 / 18);
    workspace.update_in(cx, |workspace, window, cx| {
        let doc = birchpad_core::Document::from_text(Rope::from_str(&line));
        workspace.open_document(doc, window, cx);
    });
    cx.run_until_parked();
    let view = workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap());
    let frame = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
    };
    view.update(cx, |view, cx| {
        view.selection = Selection::point(line.len() / 2);
        view.autoscroll = true;
        cx.notify();
    });
    frame(cx);
    for wrap in [false, true] {
        if wrap {
            workspace.update_in(cx, |workspace, window, cx| {
                let wrap = birchpad_commands::Invocation::new("view.word-wrap");
                workspace.dispatch(&wrap, window, cx).unwrap();
            });
            frame(cx);
            cx.run_until_parked();
            frame(cx);
        }
        let started = std::time::Instant::now();
        for _ in 0..20 {
            cx.simulate_input("x");
            frame(cx);
        }
        birchpad_core::timing::within(
            &format!("typing into a 10 MB line (word wrap {wrap}), a keystroke with a frame"),
            started.elapsed() / 20,
            std::time::Duration::from_secs(1),
        );
    }
    assert_eq!(active_text(&workspace, cx).len(), line.len() + 40);
}
