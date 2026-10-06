#![allow(clippy::single_range_in_vec_init, reason = "expected bars written out")]

use gpui_kit::{Modifiers, ScrollDelta, ScrollWheelEvent, TestAppContext, VisualTestContext};

use super::*;
use crate::panels::PanelKind;
use crate::workspace::tests::open_workspace;

const RED: u32 = 0xff0000;
const BLUE: u32 = 0x0000ff;
const TEXT: u32 = theme::TEXT;

#[test]
fn a_short_document_is_all_shown_from_the_top() {
    assert_eq!(first_map_line(10, 0..10, 100), 0);
    assert_eq!(first_map_line(100, 50..60, 100), 0, "exactly as many lines");
    assert_eq!(first_map_line(0, 0..0, 100), 0, "an empty map");
}

#[test]
fn a_long_map_scrolls_in_step_with_the_view() {
    // 1000 lines, 100 on the map, 50 in the view: the map scrolls 900 lines while the view
    // scrolls 950.
    assert_eq!(first_map_line(1000, 0..50, 100), 0);
    assert_eq!(first_map_line(1000, 950..1000, 100), 900, "at the end");
    assert_eq!(first_map_line(1000, 475..525, 100), 450, "halfway");
    // Past the end, a view taller than the document, an empty view.
    assert_eq!(first_map_line(1000, 2000..2050, 100), 900);
    assert_eq!(first_map_line(1000, 0..5000, 100), 0);
    assert_eq!(first_map_line(1000, 500..500, 100), 450);
    // A map of one line.
    assert_eq!(first_map_line(3, 2..3, 1), 2);
}

#[test]
fn bars_cover_the_runs_that_are_not_blank() {
    assert_eq!(
        line_bars("ab  cd", &[], 4, 100),
        [(0..2, TEXT), (4..6, TEXT)]
    );
    assert!(line_bars("", &[], 4, 100).is_empty());
    assert!(line_bars(" \t  ", &[], 4, 100).is_empty(), "blank");
    // A tab goes to the next stop; a wide character takes two columns.
    assert_eq!(line_bars("\tx", &[], 4, 100), [(4..5, TEXT)]);
    assert_eq!(
        line_bars("ab\tc", &[], 4, 100),
        [(0..2, TEXT), (4..5, TEXT)]
    );
    assert_eq!(line_bars("日本", &[], 4, 100), [(0..4, TEXT)]);
}

#[test]
fn bars_take_the_colors_of_the_highlights() {
    // "fn main" with "fn" red and "main" blue; the space between splits nothing more.
    let spans = [(0..2, RED), (3..7, BLUE)];
    assert_eq!(
        line_bars("fn main()", &spans, 4, 100),
        [(0..2, RED), (3..7, BLUE), (7..9, TEXT)]
    );
    // Adjacent runs of one color are one bar; a highlight over part of a word splits it.
    assert_eq!(
        line_bars("abcd", &[(1..3, RED)], 4, 100),
        [(0..1, TEXT), (1..3, RED), (3..4, TEXT)]
    );
    assert_eq!(
        line_bars("ab", &[(0..1, RED), (1..2, RED)], 4, 100),
        [(0..2, RED)]
    );
    // Highlights past the end of the line are ignored.
    assert_eq!(line_bars("a", &[(5..9, RED)], 4, 100), [(0..1, TEXT)]);
}

#[test]
fn bars_stop_at_the_edge_of_the_map() {
    assert_eq!(line_bars("abcdef", &[], 4, 4), [(0..4, TEXT)]);
    assert_eq!(
        line_bars("ab cdef", &[], 4, 4),
        [(0..2, TEXT), (3..4, TEXT)]
    );
    assert_eq!(
        line_bars("日本", &[], 4, 3),
        [(0..3, TEXT)],
        "a wide character cut in half"
    );
    assert!(line_bars("abc", &[], 4, 0).is_empty());
}

// --- In the window -------------------------------------------------------------------------

fn first_visible(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> usize {
    workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.read(cx).visible_lines().unwrap().start
    })
}

/// A document of `lines` numbered lines, without typing them.
fn open_lines(workspace: &Entity<Workspace>, lines: usize, cx: &mut VisualTestContext) {
    let text: String = (0..lines).map(|n| format!("line {n}\n")).collect();
    workspace.update_in(cx, |workspace, window, cx| {
        let doc = birchpad_core::Document::from_text(Rope::from_str(&text));
        workspace.open_document(doc, window, cx);
    });
    cx.run_until_parked();
}

fn click(at: gpui_kit::Point<Pixels>, cx: &mut VisualTestContext) {
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_map_scrolls_the_view(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    open_lines(&workspace, 150, cx);
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_panel(PanelKind::DocumentMap, window, cx);
    });
    cx.run_until_parked();
    let map = cx.debug_bounds("document-map").expect("drawn");
    assert!(
        f32::from(map.size.height) >= 150. * LINE_HEIGHT,
        "the whole document fits: {map:?}"
    );
    assert_eq!(first_visible(&workspace, cx), 0);

    // A click on line 120 of the map centers the view there.
    click(
        point(map.center().x, map.top() + px(LINE_HEIGHT * 120.5)),
        cx,
    );
    let shown = workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.read(cx).visible_lines().unwrap()
    });
    assert!(shown.contains(&120) && shown.start > 0, "{shown:?}");

    // The wheel scrolls the view.
    let before = first_visible(&workspace, cx);
    cx.simulate_event(ScrollWheelEvent {
        position: map.center(),
        delta: ScrollDelta::Lines(point(0., 3.)),
        ..Default::default()
    });
    cx.run_until_parked();
    assert_eq!(first_visible(&workspace, cx), before - 3);

    // The top of the map is the first line; below the document's last line, its end.
    click(point(map.center().x, map.top()), cx);
    assert_eq!(first_visible(&workspace, cx), 0);
    click(point(map.center().x, map.bottom() - px(1.)), cx);
    let shown = workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.read(cx).visible_lines().unwrap()
    });
    assert!(shown.contains(&149), "{shown:?}");
}

#[gpui_kit::test]
fn a_long_document_scrolls_its_map_with_the_view(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    open_lines(&workspace, 5000, cx);
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_panel(PanelKind::DocumentMap, window, cx);
        let view = workspace.active_view(cx).unwrap();
        view.update(cx, |view, cx| view.scroll_to_line(4000, cx));
    });
    cx.run_until_parked();
    let panel = workspace.read_with(cx, |workspace, _| {
        let map = workspace.docks.view(PanelKind::DocumentMap).unwrap();
        map.clone().downcast::<DocumentMap>().unwrap()
    });
    let drawn = panel.read_with(cx, |map, _| map.drawn.map(|(_, first)| first));
    let first_line = drawn.expect("drawn");
    assert!(first_line > 0 && first_line < 4000, "{first_line}");
    // A click on the map's top goes to the line the map starts with, not to the document's.
    let map = cx.debug_bounds("document-map").expect("drawn");
    click(point(map.center().x, map.top()), cx);
    let shown = workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.read(cx).visible_lines().unwrap()
    });
    assert!(
        shown.start <= first_line && first_line < shown.end,
        "{shown:?} {first_line}"
    );
}

#[gpui_kit::test]
fn scrolling_to_a_line_stays_inside_the_document(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    open_lines(&workspace, 300, cx);
    workspace.update(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.update(cx, |view, cx| view.scroll_to_line(99_999, cx));
    });
    cx.run_until_parked();
    let shown = workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.read(cx).visible_lines().unwrap()
    });
    assert!(shown.contains(&299), "the last line: {shown:?}");
}
