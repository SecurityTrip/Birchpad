//! Tests of clickable links (Ctrl+click opens them, a double click too if asked) and Ctrl+double click
//! selection between delimiters.

use gpui_kit::{
    Entity, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Point, TestAppContext,
    VisualTestContext, point, px,
};

use crate::workspace::{open_workspace, pane_views};
use birchpad::file_commands::{SHELL, Shell};
use birchpad::workspace::Workspace;
use birchpad_ui::AppState;

/// The window point of byte `pos`, in the middle of its row's height.
fn point_of(
    workspace: &Entity<Workspace>,
    pos: usize,
    cx: &mut VisualTestContext,
) -> Point<Pixels> {
    let view = pane_views(workspace, 0, cx)[0].clone();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let layout = view.layout().expect("drawn");
        let row = layout
            .rows
            .iter()
            .find(|row| row.row.range.start <= pos && pos <= row.row.range.end)
            .expect("the row is shown");
        let x = row.x_for(pos).expect("on the row");
        point(x + px(1.), row.y + layout.metrics.line_height / 2.)
    })
}

fn click(at: Point<Pixels>, clicks: usize, modifiers: Modifiers, cx: &mut VisualTestContext) {
    for click_count in 1..=clicks {
        cx.simulate_event(MouseDownEvent {
            position: at,
            modifiers,
            button: MouseButton::Left,
            click_count,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position: at,
            modifiers,
            button: MouseButton::Left,
            click_count,
        });
    }
    cx.run_until_parked();
}

fn opened() -> Vec<Shell> {
    SHELL.with(|shell| std::mem::take(&mut *shell.borrow_mut()))
}

fn selection(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> (usize, usize) {
    workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap().read(cx);
        let range = view.selection.primary();
        (range.from(), range.to())
    })
}

fn ctrl() -> Modifiers {
    Modifiers {
        control: !cfg!(target_os = "macos"),
        platform: cfg!(target_os = "macos"),
        ..Modifiers::none()
    }
}

#[gpui_kit::test]
fn links_open_with_ctrl_click_and_a_double_click_selects(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("go https://a.b/c. now");
    opened();
    let link = point_of(&workspace, 8, cx);
    // A double click on a link selects a word of it, as anywhere (issue #45).
    click(link, 2, Modifiers::none(), cx);
    assert!(opened().is_empty());
    assert_eq!(selection(&workspace, cx), (3, 8), "the word https");
    // A single click on it places the caret.
    click(link, 1, Modifiers::none(), cx);
    assert!(opened().is_empty());
    click(link, 1, ctrl(), cx);
    assert_eq!(opened(), [Shell::Browse("https://a.b/c".into())]);
    assert_eq!(
        selection(&workspace, cx).0,
        selection(&workspace, cx).1,
        "no caret added"
    );

    // Elsewhere a double click selects a word, as always.
    click(point_of(&workspace, 1, cx), 2, Modifiers::none(), cx);
    assert!(opened().is_empty());
    assert_eq!(selection(&workspace, cx), (0, 2));

    // As in Notepad++: a double click opens it too.
    cx.update(|_, cx| cx.global_mut::<AppState>().settings.links.double_click = true);
    click(point_of(&workspace, 8, cx), 2, Modifiers::none(), cx);
    assert_eq!(opened(), [Shell::Browse("https://a.b/c".into())]);

    // Links turned off: neither opens it.
    cx.update(|_, cx| cx.global_mut::<AppState>().settings.links.enabled = false);
    click(point_of(&workspace, 8, cx), 2, Modifiers::none(), cx);
    click(point_of(&workspace, 8, cx), 1, ctrl(), cx);
    assert!(opened().is_empty());
}

#[gpui_kit::test]
fn ctrl_double_click_selects_between_delimiters(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("say \"hello world\" (x) now");
    click(point_of(&workspace, 8, cx), 2, ctrl(), cx);
    assert_eq!(selection(&workspace, cx), (5, 16));

    // Other delimiters; none around the click: a word, as a double click.
    cx.update(|_, cx| {
        let delimiters = &mut cx.global_mut::<AppState>().settings.delimiters;
        delimiters.open = "(".into();
        delimiters.close = ")".into();
    });
    click(point_of(&workspace, 19, cx), 2, ctrl(), cx);
    assert_eq!(selection(&workspace, cx), (19, 20));
    click(point_of(&workspace, 1, cx), 2, ctrl(), cx);
    assert_eq!(selection(&workspace, cx), (0, 3));
    // Delimiters that are not one character each turn it off.
    cx.update(|_, cx| cx.global_mut::<AppState>().settings.delimiters.open = "((".into());
    click(point_of(&workspace, 19, cx), 2, ctrl(), cx);
    assert_eq!(selection(&workspace, cx), (19, 20), "the word x");
}
