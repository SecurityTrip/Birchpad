//! The editor view at its edges: the wheel, the scroll bars, drags past the text, input methods,
//! the clipboard, wrapped rows and several carets at the start and end of the text.

use birchpad_commands::Invocation;
use gpui_kit::{
    ClipboardItem, Entity, Modifiers, MouseButton, Pixels, Point, ScrollDelta, ScrollWheelEvent,
    TestAppContext, TouchPhase, VisualTestContext, point, px,
};

use super::*;
use crate::test_support::{Workspace, active_text, open_workspace};

fn view(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<EditorView> {
    workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap())
}

/// Replaces the text and puts the caret at its start.
fn set_text(view: &Entity<EditorView>, text: &str, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| {
        view.insert(text, LastEdit::None, cx);
        view.selection = Selection::point(0);
        cx.notify();
    });
    cx.run_until_parked();
}

fn lines(count: usize) -> String {
    (0..count).map(|line| format!("line {line}\n")).collect()
}

fn read<R>(
    view: &Entity<EditorView>,
    cx: &mut VisualTestContext,
    f: impl FnOnce(&EditorView, &Layout) -> R,
) -> R {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        f(view, view.layout.as_ref().expect("a frame was drawn"))
    })
}

fn scroll(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> (f64, Pixels) {
    view.read_with(cx, |view, _| (view.scroll_top, view.scroll_left))
}

fn wheel(at: Point<Pixels>, lines: Point<f32>, modifiers: Modifiers, cx: &mut VisualTestContext) {
    cx.simulate_event(ScrollWheelEvent {
        position: at,
        delta: ScrollDelta::Lines(lines),
        modifiers,
        touch_phase: TouchPhase::Moved,
    });
    cx.run_until_parked();
}

fn primary(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> Range {
    view.read_with(cx, |view, _| view.selection.primary())
}

fn dispatch(workspace: &Entity<Workspace>, command: &str, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace
            .dispatch(&Invocation::new(command), window, cx)
            .unwrap();
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_wheel_scrolls_and_stops_at_the_top(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let view = view(&workspace, cx);
    set_text(&view, &lines(500), cx);
    let center = read(&view, cx, |_, layout| layout.text_bounds.center());
    // Down three lines (a negative delta scrolls down), then back past the top.
    wheel(center, point(0., -3.), Modifiers::none(), cx);
    assert_eq!(scroll(&view, cx).0, 3.);
    wheel(center, point(0., 10.), Modifiers::none(), cx);
    assert_eq!(scroll(&view, cx).0, 0., "not above the first line");

    // Shift turns the wheel sideways; never left of the line start.
    set_text(&view, &"x".repeat(2000), cx);
    wheel(center, point(0., -5.), Modifiers::shift(), cx);
    let (top, left) = scroll(&view, cx);
    assert_eq!(top, 0.);
    assert!(left > px(0.));
    wheel(center, point(0., 50.), Modifiers::shift(), cx);
    assert_eq!(scroll(&view, cx).1, px(0.));
    // A sideways wheel scrolls sideways without Shift.
    wheel(center, point(-5., 0.), Modifiers::none(), cx);
    assert!(scroll(&view, cx).1 > px(0.));
}

#[gpui_kit::test]
fn ctrl_wheel_zooms_within_its_range(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let view = view(&workspace, cx);
    set_text(&view, "text", cx);
    let center = read(&view, cx, |_, layout| layout.text_bounds.center());
    let zoom = |cx: &mut VisualTestContext| cx.update(|_, cx| AppState::global(cx).state.zoom);
    let ctrl = Modifiers::secondary_key();
    wheel(center, point(0., 1.), ctrl, cx);
    assert_eq!(zoom(cx), 1);
    wheel(center, point(0., -1.), ctrl, cx);
    wheel(center, point(0., -1.), ctrl, cx);
    assert_eq!(zoom(cx), -1);
    // No movement, no change.
    wheel(center, point(0., 0.), ctrl, cx);
    assert_eq!(zoom(cx), -1);
    // Down to the smallest size and no further, up to the largest.
    for _ in 0..20 {
        wheel(center, point(0., -1.), ctrl, cx);
    }
    assert_eq!(zoom(cx), *ZOOM_RANGE.start());
    for _ in 0..60 {
        wheel(center, point(0., 1.), ctrl, cx);
    }
    assert_eq!(zoom(cx), *ZOOM_RANGE.end());
    assert_eq!(scroll(&view, cx), (0., px(0.)), "zooming does not scroll");
}

#[gpui_kit::test]
fn the_scroll_bars_drag_and_jump(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let view = view(&workspace, cx);
    set_text(&view, &lines(1000), cx);
    let (track, thumb, max) = read(&view, cx, |_, layout| {
        let bar = layout
            .vertical_bar
            .as_ref()
            .map(|bar| (bar.track, bar.thumb))
            .expect("a vertical scroll bar");
        let max = layout.total_rows.saturating_sub(layout.full_rows) as f64;
        (bar.0, bar.1, max)
    });
    // Dragging the thumb to the bottom of the track scrolls to the end, and past it no further.
    let grab = thumb.center();
    cx.simulate_mouse_down(grab, MouseButton::Left, Modifiers::none());
    let below = point(grab.x, track.bottom() - px(1.));
    cx.simulate_mouse_move(below, MouseButton::Left, Modifiers::none());
    assert_eq!(scroll(&view, cx).0, max);
    // And to the top.
    let above = point(grab.x, track.top() + px(1.));
    cx.simulate_mouse_move(above, MouseButton::Left, Modifiers::none());
    assert_eq!(scroll(&view, cx).0, 0.);
    cx.simulate_mouse_up(above, MouseButton::Left, Modifiers::none());
    // A click in the track below the thumb jumps there, the thumb centered under the pointer.
    let low = point(grab.x, track.bottom() - px(2.));
    cx.simulate_mouse_down(low, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(low, MouseButton::Left, Modifiers::none());
    let top = scroll(&view, cx).0;
    assert!(top > max * 0.9 && top <= max, "{top} of {max}");
    // The selection did not move.
    assert_eq!(primary(&view, cx), Range::point(0));

    // The horizontal bar of a long line.
    set_text(&view, &"y".repeat(3000), cx);
    let bar = read(&view, cx, |_, layout| {
        layout
            .horizontal_bar
            .as_ref()
            .map(|bar| (bar.track, bar.thumb))
    });
    let (track, thumb) = bar.expect("a horizontal scroll bar");
    let grab = thumb.center();
    cx.simulate_mouse_down(grab, MouseButton::Left, Modifiers::none());
    let right = point(track.right() - px(1.), grab.y);
    cx.simulate_mouse_move(right, MouseButton::Left, Modifiers::none());
    assert!(scroll(&view, cx).1 > px(0.));
    let left = point(track.left() + px(1.), grab.y);
    cx.simulate_mouse_move(left, MouseButton::Left, Modifiers::none());
    assert_eq!(scroll(&view, cx).1, px(0.));
    cx.simulate_mouse_up(left, MouseButton::Left, Modifiers::none());
    assert_eq!(primary(&view, cx), Range::point(0));
}

#[gpui_kit::test]
fn dragging_past_the_top_and_bottom_scrolls(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let view = view(&workspace, cx);
    set_text(&view, &lines(500), cx);
    let (inside, below, above) = read(&view, cx, |_, layout| {
        let bounds = layout.text_bounds;
        let x = bounds.left() + px(20.);
        (
            point(x, bounds.top() + px(5.)),
            point(x, bounds.bottom() + px(10.)),
            point(x, bounds.top() - px(10.)),
        )
    });
    cx.simulate_mouse_down(inside, MouseButton::Left, Modifiers::none());
    // Also outside the window, as while the button is held past its edge.
    let drag_to = |position, pressed: Option<MouseButton>, cx: &mut VisualTestContext| {
        view.update_in(cx, |view, window, cx| {
            let event = gpui_kit::MouseMoveEvent {
                position,
                pressed_button: pressed,
                modifiers: Modifiers::none(),
            };
            view.mouse_move(&event, window, cx);
        });
        cx.run_until_parked();
    };
    for _ in 0..5 {
        drag_to(below, Some(MouseButton::Left), cx);
    }
    assert_eq!(scroll(&view, cx).0, 5.);
    assert!(primary(&view, cx).head > 0, "the selection follows");
    for _ in 0..10 {
        drag_to(above, Some(MouseButton::Left), cx);
    }
    assert_eq!(scroll(&view, cx).0, 0., "not above the first line");
    // A move without the button held ends the drag.
    let selected = primary(&view, cx);
    drag_to(below, None, cx);
    drag_to(below, Some(MouseButton::Left), cx);
    assert_eq!(primary(&view, cx), selected);
    assert_eq!(scroll(&view, cx).0, 0.);
}

#[gpui_kit::test]
fn input_methods_compose_in_place(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let view = view(&workspace, cx);
    // "𝄞" is two UTF-16 units and four bytes; positions are given in UTF-16.
    set_text(&view, "a𝄞b", cx);
    view.update_in(cx, |view, window, cx| {
        view.selection = Selection::single(Range::new(1, 5));
        let selected = view.selected_text_range(false, window, cx).unwrap();
        assert_eq!((selected.range, selected.reversed), (1..3, false));
        let mut adjusted = None;
        assert_eq!(
            view.text_for_range(1..3, &mut adjusted, window, cx)
                .as_deref(),
            Some("𝄞")
        );
        assert_eq!(adjusted, Some(1..3));
        // Out of range is clamped to the end.
        assert_eq!(
            view.text_for_range(3..99, &mut adjusted, window, cx)
                .as_deref(),
            Some("b")
        );
        assert_eq!(view.marked_text_range(window, cx), None);

        // Composing replaces the selection and marks what is being composed.
        view.replace_and_mark_text_in_range(None, "かな", Some(1..2), window, cx);
        assert_eq!(view.text(cx).to_string(), "aかなb");
        assert_eq!(view.marked_text_range(window, cx), Some(1..3));
        assert_eq!(view.selection.primary(), Range::new(4, 7));
        // The next step of the composition replaces the marked text.
        view.replace_and_mark_text_in_range(None, "漢字", None, window, cx);
        assert_eq!(view.text(cx).to_string(), "a漢字b");
        assert_eq!(view.marked_text_range(window, cx), Some(1..3));
        // Committing replaces it once more and unmarks it.
        view.replace_text_in_range(None, "感じ", window, cx);
        assert_eq!(view.text(cx).to_string(), "a感じb");
        assert_eq!(view.marked_text_range(window, cx), None);

        // An empty composition marks nothing; unmarking keeps the text.
        view.replace_and_mark_text_in_range(Some(0..1), "", None, window, cx);
        assert_eq!(view.marked_text_range(window, cx), None);
        assert_eq!(view.text(cx).to_string(), "感じb");
        view.replace_and_mark_text_in_range(Some(2..3), "x", Some(1..1), window, cx);
        view.unmark_text(window, cx);
        assert_eq!(view.marked_text_range(window, cx), None);
        assert_eq!(view.text(cx).to_string(), "感じx");
    });
    assert_eq!(active_text(&workspace, cx), "感じx");
}

#[gpui_kit::test]
fn input_methods_find_text_on_screen(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let view = view(&workspace, cx);
    set_text(&view, "a𝄞bc\nsecond", cx);
    let (first, second) = read(&view, cx, |_, layout| {
        let (first, second) = (&layout.rows[0], &layout.rows[1]);
        let middle = layout.metrics.line_height / 2.;
        (
            point(first.x_for(5).unwrap() + px(1.), first.y + middle),
            point(
                second.x_for(second.row.range.start).unwrap(),
                second.y + middle,
            ),
        )
    });
    view.update_in(cx, |view, window, cx| {
        let element = Bounds::default();
        // "b": UTF-16 3..4.
        let bounds = view.bounds_for_range(3..4, element, window, cx).unwrap();
        assert!(bounds.size.width > px(0.));
        assert!(
            (bounds.left() - first.x).abs() < px(2.),
            "{bounds:?} {first:?}"
        );
        assert_eq!(view.character_index_for_point(first, window, cx), Some(3));
        // The second line starts after "a𝄞bc\n": six UTF-16 units.
        assert_eq!(view.character_index_for_point(second, window, cx), Some(6));
    });
}

#[gpui_kit::test]
fn pasting_nothing_or_no_text_changes_nothing(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let view = view(&workspace, cx);
    set_text(&view, "keep", cx);
    // An empty clipboard.
    dispatch(&workspace, "edit.paste", cx);
    assert_eq!(active_text(&workspace, cx), "keep");
    // An image.
    let image =
        gpui_kit::Image::from_bytes(gpui_kit::ImageFormat::Png, vec![0x89, b'P', b'N', b'G']);
    cx.write_to_clipboard(ClipboardItem::new_image(&image));
    dispatch(&workspace, "edit.paste", cx);
    assert_eq!(active_text(&workspace, cx), "keep");
    // Text, at the caret.
    cx.write_to_clipboard(ClipboardItem::new_string("x".into()));
    dispatch(&workspace, "edit.paste", cx);
    assert_eq!(active_text(&workspace, cx), "xkeep");
    // Cut with nothing selected cuts nothing (Copy Line is a command of its own).
    view.update(cx, |view, _| view.selection = Selection::point(5));
    cx.write_to_clipboard(ClipboardItem::new_string("before".into()));
    dispatch(&workspace, "edit.cut", cx);
    assert_eq!(active_text(&workspace, cx), "xkeep");
}

#[gpui_kit::test]
fn clicks_in_wrapped_rows_land_in_their_part_of_the_line(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let view = view(&workspace, cx);
    let line = "word ".repeat(60);
    set_text(&view, &format!("{line}\nlast"), cx);
    dispatch(&workspace, "view.word-wrap", cx);
    let (second_row, second_start, last_row) = read(&view, cx, |_, layout| {
        assert!(layout.rows.len() > 2, "the long line wraps");
        let middle = layout.metrics.line_height / 2.;
        let second = &layout.rows[1];
        let last = layout.rows.iter().find(|row| row.row.line == 1).unwrap();
        (
            point(
                second.x_for(second.row.range.start).unwrap() + px(1.),
                second.y + middle,
            ),
            second.row.range.start,
            point(layout.text_bounds.right() - px(2.), last.y + middle),
        )
    });
    assert!(second_start > 0, "the second row continues the first line");
    cx.simulate_click(second_row, Modifiers::none());
    let caret = primary(&view, cx).head;
    assert!(
        (second_start..second_start + 2).contains(&caret),
        "{caret} near {second_start}"
    );
    // Right of the end of the last line: its end.
    cx.simulate_click(last_row, Modifiers::none());
    assert_eq!(primary(&view, cx), Range::point(line.len() + 5));
    // Down from the last row stays on it, at the end of the text.
    cx.simulate_keystrokes("down");
    assert_eq!(primary(&view, cx), Range::point(line.len() + 5));
    // Up goes back into the wrapped line, not to its start.
    cx.simulate_keystrokes("up");
    let up = primary(&view, cx).head;
    assert!(up > second_start && up <= line.len(), "{up}");
}

#[gpui_kit::test]
fn several_carets_at_the_edges_of_the_text(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let view = view(&workspace, cx);
    set_text(&view, "ab\ncd\nef", cx);
    let carets = |view: &Entity<EditorView>, cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.selection
                .iter()
                .map(|range| (range.anchor, range.head))
                .collect::<Vec<_>>()
        })
    };
    // A caret at the start of each line: Left keeps the first at the start of the text.
    view.update(cx, |view, cx| {
        view.selection = Selection::new([Range::point(0), Range::point(3), Range::point(6)], 0);
        cx.notify();
    });
    cx.simulate_keystrokes("left");
    assert_eq!(carets(&view, cx), [(0, 0), (2, 2), (5, 5)]);
    // Backspace at the start of the text deletes only where there is something before.
    cx.simulate_keystrokes("backspace");
    assert_eq!(active_text(&workspace, cx), "a\nc\nef");

    // A caret at the end of each line: Right keeps the last at the end of the text.
    let ends = Selection::new([Range::point(1), Range::point(3), Range::point(6)], 2);
    view.update(cx, |view, cx| {
        view.selection = ends.clone();
        cx.notify();
    });
    cx.simulate_keystrokes("right");
    assert_eq!(carets(&view, cx), [(2, 2), (4, 4), (6, 6)]);
    // Delete deletes the line breaks, and nothing at the end of the text.
    view.update(cx, |view, cx| {
        view.selection = ends;
        cx.notify();
    });
    cx.simulate_keystrokes("delete");
    assert_eq!(active_text(&workspace, cx), "acef");

    // Typing at the first and the last position at once.
    view.update(cx, |view, cx| {
        view.selection = Selection::new([Range::point(0), Range::point(4)], 0);
        cx.notify();
    });
    cx.simulate_input("-");
    assert_eq!(active_text(&workspace, cx), "-acef-");
    assert_eq!(carets(&view, cx), [(1, 1), (6, 6)]);
    // Shift+Home from both selects back to the line start: one selection.
    cx.simulate_keystrokes("shift-home");
    assert_eq!(
        view.read_with(cx, |view, _| view.selection.iter().count()),
        1
    );
    assert_eq!(primary(&view, cx).from(), 0);
}
