//! The find panel in a window of its own (ADR 0070): opened by Ctrl+F, searching in the main
//! window from there, its keys going on to the main window, docked and floated again by its
//! button, closed by Escape and by its close button.

use std::fs;

use gpui_kit::{
    AnyWindowHandle, Bounds, Entity, MouseButton, TestAppContext, VisualTestContext, point, px,
    size,
};

use crate::workspace::{click_on, document_start, open_workspace, secondary};
use birchpad::app_state::AppState;
use birchpad::find::window::placement;
use birchpad::find::*;
use birchpad::workspace::Workspace;

const TEXT: &str = "cat Cat category\ncat";

/// The panel set to float, written to a settings file of the test's own, which the panel's
/// button writes too.
fn floating(cx: &mut VisualTestContext) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    // Messages in English, whatever the system's language: the settings are read again.
    fs::write(&path, "[appearance]\nlanguage = \"en\"\n").unwrap();
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().paths.user_settings = Some(path);
        birchpad::preferences::set(
            "searching.find-panel-floats",
            Some(toml::Value::Boolean(true)),
            cx,
        )
        .unwrap();
    });
    dir
}

fn find_window(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> Option<AnyWindowHandle> {
    workspace.read_with(cx, |workspace, _| workspace.find_panel_window())
}

fn bar<R>(
    workspace: &Entity<Workspace>,
    read: impl FnOnce(&FindBar, &gpui_kit::App) -> R,
    cx: &mut VisualTestContext,
) -> R {
    workspace.read_with(cx, |workspace, cx| read(workspace.find_bar.read(cx), cx))
}

fn selection(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> (usize, usize) {
    workspace.read_with(cx, |workspace, cx| {
        let range = workspace
            .active_view(cx)
            .unwrap()
            .read(cx)
            .selection
            .primary();
        (range.anchor, range.head)
    })
}

fn floats(cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| FindBar::floats(cx))
}

/// The text typed in the main window, the caret at its start, and Ctrl+F: the panel's window,
/// and a test context for it.
fn open_find(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> (AnyWindowHandle, VisualTestContext) {
    cx.simulate_input(TEXT);
    cx.simulate_keystrokes(document_start());
    cx.simulate_keystrokes(&secondary("f"));
    let handle = find_window(workspace, cx).expect("the panel's window is open");
    (handle, VisualTestContext::from_window(handle, cx))
}

#[gpui_kit::test]
fn the_panel_opens_in_a_window_of_its_own_and_searches_from_there(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let _settings = floating(cx);
    let (handle, mut find) = open_find(&workspace, cx);

    // A second window, not the docked panel in the first.
    assert_eq!(cx.windows().len(), 2);
    assert!(cx.update(|_, cx| is_find_window(handle, cx)));
    let main = gpui_kit::VisualContext::window_handle(cx);
    assert!(!cx.update(|_, cx| is_find_window(main, cx)));
    assert!(cx.debug_bounds("find-bar").is_none(), "not docked");
    assert!(find.debug_bounds("find-bar").is_some());

    // Typed into its find field, Enter finds in the main window's document.
    find.simulate_input("cat");
    find.simulate_keystrokes("enter");
    assert_eq!(selection(&workspace, cx), (0, 3));
    find.simulate_keystrokes("enter");
    assert_eq!(selection(&workspace, cx), (4, 7));
    // F3, a key of the main window's commands, works from here too.
    find.simulate_keystrokes("f3");
    find.run_until_parked();
    assert_eq!(selection(&workspace, cx), (8, 11));
    // Its buttons, in a column at the right.
    click_on("count", MouseButton::Left, &mut find);
    assert_eq!(
        bar(&workspace, |bar, _| bar.status_text(), cx).as_deref(),
        Some("Count: 4 matches in entire file")
    );

    // Ctrl+F again does not open another window.
    cx.simulate_keystrokes(&secondary("f"));
    assert_eq!(cx.windows().len(), 2);
    assert_eq!(find_window(&workspace, cx), Some(handle));

    // Escape closes it, and the panel.
    find.simulate_keystrokes("escape");
    find.run_until_parked();
    cx.run_until_parked();
    assert_eq!(find_window(&workspace, cx), None);
    assert_eq!(cx.windows().len(), 1);
    assert!(!bar(&workspace, |bar, _| bar.visible, cx));

    // Opened again: what it held is there.
    cx.simulate_keystrokes(&secondary("f"));
    assert_eq!(cx.windows().len(), 2);
    assert_eq!(bar(&workspace, |bar, cx| bar.query(cx).pattern, cx), "cat");
}

#[gpui_kit::test]
fn the_button_docks_the_panel_and_floats_it_again(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let settings = floating(cx);
    let (_, mut find) = open_find(&workspace, cx);
    find.simulate_input("cat");
    click_on("match-case", MouseButton::Left, &mut find);

    // Docked: the window closes, the panel is under the documents with what it held, and the
    // settings keep it.
    click_on("float-find", MouseButton::Left, &mut find);
    cx.run_until_parked();
    assert!(!floats(cx));
    assert_eq!(find_window(&workspace, cx), None);
    assert_eq!(cx.windows().len(), 1);
    assert!(cx.debug_bounds("find-bar").is_some());
    assert!(bar(&workspace, |bar, _| bar.visible, cx));
    assert_eq!(bar(&workspace, |bar, cx| bar.query(cx).pattern, cx), "cat");
    assert!(bar(&workspace, |bar, _| bar.options.match_case, cx));
    let text = fs::read_to_string(settings.path().join("settings.toml")).unwrap();
    assert!(text.contains("find-panel-floats = false"), "{text}");
    click_on("find-next", MouseButton::Left, cx);
    assert_eq!(selection(&workspace, cx), (0, 3));
    click_on("find-next", MouseButton::Left, cx);
    assert_eq!(
        selection(&workspace, cx),
        (8, 11),
        "not Cat, which differs in case"
    );

    // And floating again: a window again, with the same.
    click_on("float-find", MouseButton::Left, cx);
    cx.run_until_parked();
    assert!(floats(cx));
    let handle = find_window(&workspace, cx).expect("a window again");
    assert_eq!(cx.windows().len(), 2);
    assert!(cx.debug_bounds("find-bar").is_none());
    let mut find = VisualTestContext::from_window(handle, cx);
    assert!(find.debug_bounds("find-bar").is_some());
    assert_eq!(bar(&workspace, |bar, cx| bar.query(cx).pattern, cx), "cat");
    let text = fs::read_to_string(settings.path().join("settings.toml")).unwrap();
    assert!(text.contains("find-panel-floats = true"), "{text}");
}

#[gpui_kit::test]
fn the_close_button_of_the_window_closes_the_panel(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let _settings = floating(cx);
    let (_, mut find) = open_find(&workspace, cx);
    find.simulate_input("cat");
    assert!(find.simulate_close(), "the window may close");
    find.update(|window, _| window.remove_window());
    cx.run_until_parked();
    assert_eq!(find_window(&workspace, cx), None);
    assert!(!bar(&workspace, |bar, _| bar.visible, cx));
    // F3 still repeats the search from the main window, as with the panel closed.
    cx.simulate_keystrokes("f3");
    assert_eq!(selection(&workspace, cx), (0, 3));
    // Ctrl+F opens a new window.
    cx.simulate_keystrokes(&secondary("f"));
    assert!(find_window(&workspace, cx).is_some());
    assert_eq!(cx.windows().len(), 2);
}

#[gpui_kit::test]
fn without_a_settings_file_the_panel_says_why_it_stays_docked(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.update(|_, cx| cx.global_mut::<AppState>().paths.user_settings = None);
    cx.simulate_input(TEXT);
    cx.simulate_keystrokes(&secondary("f"));
    click_on("float-find", MouseButton::Left, cx);
    assert!(!floats(cx), "nothing changed");
    assert_eq!(find_window(&workspace, cx), None);
    assert_eq!(cx.windows().len(), 1);
    assert!(cx.debug_bounds("find-bar").is_some());
    assert!(bar(
        &workspace,
        |bar, _| bar.status.as_ref().is_some_and(|(_, failed)| *failed),
        cx
    ));
}

#[test]
fn the_window_opens_near_the_top_right_corner_of_the_main_one() {
    let main = Bounds::new(point(px(100.), px(50.)), size(px(1200.), px(800.)));
    let at = placement(main);
    assert_eq!(at.size, size(px(760.), px(300.)));
    assert_eq!(
        at.right(),
        main.right() - px(32.),
        "a little in from the right"
    );
    assert_eq!(
        at.top(),
        main.top() + px(96.),
        "below the menus and the tabs"
    );
    // A main window narrower than the panel: from its left edge, not off it.
    let narrow = Bounds::new(point(px(100.), px(50.)), size(px(500.), px(400.)));
    assert_eq!(placement(narrow).left(), narrow.left());
    // Exactly as wide as the panel and its margin: at the left edge.
    let fits = Bounds::new(point(px(0.), px(0.)), size(px(792.), px(400.)));
    assert_eq!(placement(fits).left(), px(0.));
}
