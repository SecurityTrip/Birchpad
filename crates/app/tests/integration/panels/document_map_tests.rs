#![allow(clippy::single_range_in_vec_init, reason = "expected bars written out")]

use gpui_kit::{Modifiers, ScrollDelta, ScrollEvent, TestAppContext, VisualTestContext};

use crate::workspace::open_workspace;
use birchpad::panels::PanelKind;
use birchpad::panels::document_map::*;
use birchpad::workspace::Workspace;
use birchpad_core::Rope;
use gpui_kit::Entity;
use gpui_kit::MouseButton;
use gpui_kit::Pixels;
use gpui_kit::point;
use gpui_kit::px;

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
    cx.simulate_event(ScrollEvent {
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
