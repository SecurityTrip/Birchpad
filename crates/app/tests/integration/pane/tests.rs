//! Tests of the tab bar: pinned and colored tabs, moving tabs, going to a tab, closing several
//! documents, the document switcher, the tab's context menu, the document peeker, the tab
//! bar's settings and tabs in sessions.

use std::fs;

use anyhow::Result;
use birchpad_commands::Invocation;
use birchpad_config::TabSettings;
use birchpad_core::Rope;
use gpui_kit::{
    Bounds, Entity, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Point,
    TestAppContext, VisualTestContext, point,
};
use serde_json::json;

use crate::workspace::{click_on, open_workspace, pane_views, secondary, tab_names};
use birchpad::app_state::AppState;
use birchpad::pane::{PaneEvent, TabMarks, peek_lines};
use birchpad::workspace::Workspace;

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

fn command(workspace: &Entity<Workspace>, id: &str, cx: &mut VisualTestContext) {
    run(workspace, Invocation::new(id), cx).unwrap();
}

/// `n` documents, "new 1" to "new n", the last one active.
fn documents(n: usize, cx: &mut TestAppContext) -> (Entity<Workspace>, &mut VisualTestContext) {
    let (workspace, cx) = open_workspace(cx);
    for _ in 1..n {
        cx.simulate_keystrokes(&secondary("n"));
    }
    (workspace, cx)
}

fn active_name(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.read(cx).buffer.read(cx).display_name()
    })
}

/// The tabs of the main view: name, and `*` for pinned or the color's number.
fn tabs(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    pane_views(workspace, 0, cx)
        .iter()
        .map(|view| {
            view.read_with(cx, |view, cx| {
                let mut name = view.buffer.read(cx).display_name();
                if view.tab.pinned {
                    name.push('*');
                }
                if let Some(color) = view.tab.color {
                    name.push_str(&format!(" {color}"));
                }
                name
            })
        })
        .collect()
}

fn go_to(workspace: &Entity<Workspace>, index: usize, cx: &mut VisualTestContext) {
    run(
        workspace,
        Invocation::with_args("view.go-to-tab", json!({ "index": index })),
        cx,
    )
    .unwrap();
}

fn tab_settings(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
    change: impl FnOnce(&mut TabSettings),
) {
    cx.update(|_, cx| change(&mut cx.global_mut::<AppState>().settings.tabs));
    workspace.update(cx, |workspace, cx| workspace.refresh_views(cx));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

fn bounds(selector: String, cx: &mut VisualTestContext) -> Option<Bounds<Pixels>> {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    cx.debug_bounds(Box::leak(selector.into_boxed_str()))
}

fn tab_bounds(index: usize, cx: &mut VisualTestContext) -> Bounds<Pixels> {
    bounds(format!("pane-0-tab-{index}"), cx).expect("the tab is drawn")
}

fn double_click(at: Point<Pixels>, cx: &mut VisualTestContext) {
    for click_count in [1, 2] {
        cx.simulate_event(MouseDownEvent {
            position: at,
            modifiers: Modifiers::none(),
            button: MouseButton::Left,
            click_count,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position: at,
            modifiers: Modifiers::none(),
            button: MouseButton::Left,
            click_count,
        });
    }
    cx.run_until_parked();
}

fn drag(from: Point<Pixels>, to: Point<Pixels>, cx: &mut VisualTestContext) {
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    for step in 1..=4 {
        let t = step as f32 / 4.;
        let at = point(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
        cx.simulate_mouse_move(at, MouseButton::Left, Modifiers::none());
    }
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

fn release_ctrl(cx: &mut VisualTestContext) {
    cx.simulate_modifiers_change(Modifiers::none());
    cx.run_until_parked();
}

fn switcher_shown(cx: &mut VisualTestContext) -> bool {
    bounds("pane-0-switcher".into(), cx).is_some()
}

#[gpui_kit::test]
fn pinned_tabs_stay_at_the_start(cx: &mut TestAppContext) {
    let (workspace, cx) = documents(4, cx);
    // One tab pinned: it goes first and stays active.
    command(&workspace, "view.pin-tab", cx);
    assert_eq!(tabs(&workspace, cx), ["new 4*", "new 1", "new 2", "new 3"]);
    assert_eq!(active_name(&workspace, cx), "new 4");

    // A second one goes after it; a new document after the pinned ones.
    go_to(&workspace, 3, cx);
    command(&workspace, "view.pin-tab", cx);
    assert_eq!(tabs(&workspace, cx), ["new 4*", "new 2*", "new 1", "new 3"]);
    cx.simulate_keystrokes(&secondary("n"));
    assert_eq!(
        tabs(&workspace, cx),
        ["new 4*", "new 2*", "new 5", "new 1", "new 3"]
    );

    // Dropped among the pinned tabs, a tab that is not pinned stays after them, and a pinned
    // one dropped at the end stays with the pinned ones.
    let views = pane_views(&workspace, 0, cx);
    let pane = workspace.read_with(cx, |workspace, _| workspace.panes()[0].clone());
    for (view, index) in [(views[4].clone(), 0), (views[0].clone(), 5)] {
        pane.update(cx, |_, cx| {
            cx.emit(PaneEvent::Dropped {
                view,
                index,
                clone: false,
            })
        });
        cx.run_until_parked();
    }
    assert_eq!(
        tabs(&workspace, cx),
        ["new 2*", "new 4*", "new 3", "new 5", "new 1"]
    );

    // Unpinned, a tab goes to the start of the others.
    go_to(&workspace, 1, cx);
    command(&workspace, "view.pin-tab", cx);
    assert_eq!(
        tabs(&workspace, cx),
        ["new 4*", "new 2", "new 3", "new 5", "new 1"]
    );
    assert_eq!(active_name(&workspace, cx), "new 2");

    // The pin on the tab unpins it.
    click_on("pane-0-unpin-tab-0", MouseButton::Left, cx);
    assert_eq!(
        tabs(&workspace, cx),
        ["new 4", "new 2", "new 3", "new 5", "new 1"]
    );
}

#[gpui_kit::test]
fn every_tab_pinned_or_the_only_one(cx: &mut TestAppContext) {
    let (workspace, cx) = documents(1, cx);
    command(&workspace, "view.pin-tab", cx);
    assert_eq!(tabs(&workspace, cx), ["new 1*"]);
    // Moving the only tab, or the only pinned one, changes nothing.
    for id in [
        "view.move-tab-forward",
        "view.move-tab-backward",
        "view.move-tab-to-start",
        "view.move-tab-to-end",
    ] {
        command(&workspace, id, cx);
    }
    assert_eq!(tabs(&workspace, cx), ["new 1*"]);
    cx.simulate_keystrokes(&secondary("n"));
    command(&workspace, "view.pin-tab", cx);
    assert_eq!(tabs(&workspace, cx), ["new 1*", "new 2*"]);
    command(&workspace, "view.move-tab-to-start", cx);
    assert_eq!(tabs(&workspace, cx), ["new 2*", "new 1*"]);
}

#[gpui_kit::test]
fn tabs_move_within_their_group(cx: &mut TestAppContext) {
    let (workspace, cx) = documents(4, cx);
    // The last tab: forward stays, backward goes left.
    command(&workspace, "view.move-tab-forward", cx);
    assert_eq!(tabs(&workspace, cx), ["new 1", "new 2", "new 3", "new 4"]);
    cx.simulate_keystrokes("ctrl-shift-pageup");
    assert_eq!(tabs(&workspace, cx), ["new 1", "new 2", "new 4", "new 3"]);
    assert_eq!(active_name(&workspace, cx), "new 4");
    command(&workspace, "view.move-tab-to-start", cx);
    assert_eq!(tabs(&workspace, cx), ["new 4", "new 1", "new 2", "new 3"]);
    command(&workspace, "view.move-tab-backward", cx);
    assert_eq!(tabs(&workspace, cx), ["new 4", "new 1", "new 2", "new 3"]);
    cx.simulate_keystrokes("ctrl-shift-pagedown");
    assert_eq!(tabs(&workspace, cx), ["new 1", "new 4", "new 2", "new 3"]);
    command(&workspace, "view.move-tab-to-end", cx);
    assert_eq!(tabs(&workspace, cx), ["new 1", "new 2", "new 3", "new 4"]);

    // A tab that is not pinned does not pass the pinned ones.
    go_to(&workspace, 1, cx);
    command(&workspace, "view.pin-tab", cx);
    go_to(&workspace, 3, cx);
    command(&workspace, "view.move-tab-to-start", cx);
    assert_eq!(tabs(&workspace, cx), ["new 1*", "new 3", "new 2", "new 4"]);
    command(&workspace, "view.move-tab-backward", cx);
    assert_eq!(tabs(&workspace, cx), ["new 1*", "new 3", "new 2", "new 4"]);
}

#[gpui_kit::test]
fn going_to_a_tab_by_its_number(cx: &mut TestAppContext) {
    let (workspace, cx) = documents(3, cx);
    go_to(&workspace, 1, cx);
    assert_eq!(active_name(&workspace, cx), "new 1");
    go_to(&workspace, 3, cx);
    assert_eq!(active_name(&workspace, cx), "new 3");
    go_to(&workspace, 2, cx);
    // Past the last tab: nothing happens.
    go_to(&workspace, 9, cx);
    go_to(&workspace, 4, cx);
    assert_eq!(active_name(&workspace, cx), "new 2");
    command(&workspace, "view.last-tab", cx);
    assert_eq!(active_name(&workspace, cx), "new 3");

    // Tab 0, or no number: an error, and nothing changes.
    let error = run(
        &workspace,
        Invocation::with_args("view.go-to-tab", json!({ "index": 0 })),
        cx,
    )
    .unwrap_err();
    assert!(error.to_string().contains("from 1"), "{error}");
    assert!(run(&workspace, Invocation::new("view.go-to-tab"), cx).is_err());
    assert!(
        run(
            &workspace,
            Invocation::with_args("view.go-to-tab", json!({ "index": -1 })),
            cx
        )
        .is_err()
    );
    assert_eq!(active_name(&workspace, cx), "new 3");
}

#[gpui_kit::test]
fn tab_colors(cx: &mut TestAppContext) {
    let (workspace, cx) = documents(2, cx);
    let color = |color: i64| Invocation::with_args("view.tab-color", json!({ "color": color }));
    run(&workspace, color(1), cx).unwrap();
    assert_eq!(tabs(&workspace, cx), ["new 1", "new 2 1"]);
    run(&workspace, color(5), cx).unwrap();
    assert_eq!(tabs(&workspace, cx), ["new 1", "new 2 5"]);

    // The menu checks the active tab's color and pin.
    let marks = Some(TabMarks {
        pinned: false,
        color: Some(5),
    });
    assert_eq!(birchpad::pane::is_checked(&color(5), marks), Some(true));
    assert_eq!(birchpad::pane::is_checked(&color(1), marks), Some(false));
    assert_eq!(
        birchpad::pane::is_checked(&Invocation::new("view.pin-tab"), marks),
        Some(false)
    );
    assert_eq!(birchpad::pane::is_checked(&color(1), None), Some(false));
    assert_eq!(
        birchpad::pane::is_checked(&Invocation::new("file.new"), marks),
        None
    );

    // Colors past the ends are refused and change nothing.
    for wrong in [0, 6, -1] {
        assert!(run(&workspace, color(wrong), cx).is_err(), "{wrong}");
    }
    let error = run(&workspace, color(6), cx).unwrap_err();
    assert!(error.to_string().contains("1 to 5"), "{error}");
    assert_eq!(tabs(&workspace, cx), ["new 1", "new 2 5"]);
    command(&workspace, "view.remove-tab-color", cx);
    assert_eq!(tabs(&workspace, cx), ["new 1", "new 2"]);
    // Removing no color is no error.
    command(&workspace, "view.remove-tab-color", cx);
    assert_eq!(tabs(&workspace, cx), ["new 1", "new 2"]);
}

#[gpui_kit::test]
fn closing_several_documents(cx: &mut TestAppContext) {
    let (workspace, cx) = documents(5, cx);
    go_to(&workspace, 3, cx);
    command(&workspace, "file.close-left", cx);
    assert_eq!(tab_names(&workspace, cx), ["new 3", "new 4", "new 5"]);
    // The first tab has nothing on its left; the last nothing on its right.
    command(&workspace, "file.close-left", cx);
    assert_eq!(tab_names(&workspace, cx), ["new 3", "new 4", "new 5"]);
    command(&workspace, "file.close-right", cx);
    assert_eq!(tab_names(&workspace, cx), ["new 3"]);
    command(&workspace, "file.close-right", cx);
    assert_eq!(tab_names(&workspace, cx), ["new 3"]);

    // Close All Unchanged leaves documents with changes.
    cx.simulate_input("changed");
    cx.simulate_keystrokes(&secondary("n"));
    cx.simulate_keystrokes(&secondary("n"));
    command(&workspace, "file.close-unchanged", cx);
    assert!(!cx.has_pending_prompt());
    assert_eq!(tab_names(&workspace, cx), ["new 3"]);

    // Close All but Pinned asks about changes; Cancel keeps the rest.
    cx.simulate_keystrokes(&secondary("n"));
    command(&workspace, "view.pin-tab", cx);
    cx.simulate_keystrokes(&secondary("n"));
    assert_eq!(tab_names(&workspace, cx), ["new 1", "new 2", "new 3"]);
    command(&workspace, "file.close-all-but-pinned", cx);
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx), ["new 1", "new 3"]);
    command(&workspace, "file.close-all-but-pinned", cx);
    cx.simulate_prompt_answer("Don't Save");
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx), ["new 1"]);
}

#[gpui_kit::test]
fn the_document_switcher_goes_by_recent_use(cx: &mut TestAppContext) {
    let (workspace, cx) = documents(4, cx);
    // Used last: 4, then 1, then 3.
    go_to(&workspace, 3, cx);
    go_to(&workspace, 1, cx);
    go_to(&workspace, 4, cx);
    assert!(!switcher_shown(cx));

    // While Ctrl is held, each Ctrl+Tab goes one further down the list.
    cx.simulate_keystrokes("ctrl-tab");
    assert_eq!(active_name(&workspace, cx), "new 1");
    assert!(switcher_shown(cx));
    cx.simulate_keystrokes("ctrl-tab");
    assert_eq!(active_name(&workspace, cx), "new 3");
    cx.simulate_keystrokes("ctrl-shift-tab");
    assert_eq!(active_name(&workspace, cx), "new 1");
    cx.simulate_keystrokes("ctrl-tab ctrl-tab ctrl-tab");
    assert_eq!(active_name(&workspace, cx), "new 4", "around the list");
    cx.simulate_keystrokes("ctrl-tab ctrl-tab");
    assert_eq!(active_name(&workspace, cx), "new 3");

    // Released, the document stays and is now the most recent: Ctrl+Tab goes back.
    release_ctrl(cx);
    assert!(!switcher_shown(cx));
    assert_eq!(active_name(&workspace, cx), "new 3");
    cx.simulate_keystrokes("ctrl-tab");
    assert_eq!(active_name(&workspace, cx), "new 4");
    release_ctrl(cx);

    // Backward from the start: the least recent one.
    cx.simulate_keystrokes("ctrl-shift-tab");
    assert_eq!(active_name(&workspace, cx), "new 2");
    release_ctrl(cx);

    // A click in the list goes there.
    cx.simulate_keystrokes("ctrl-tab");
    click_on("pane-0-switcher-2", MouseButton::Left, cx);
    assert!(!switcher_shown(cx));
    assert_eq!(active_name(&workspace, cx), "new 3");
}

#[gpui_kit::test]
fn the_switcher_in_tab_order_or_turned_off(cx: &mut TestAppContext) {
    let (workspace, cx) = documents(3, cx);
    go_to(&workspace, 2, cx);

    tab_settings(&workspace, cx, |tabs| tabs.switcher_mru = false);
    cx.simulate_keystrokes("ctrl-tab");
    assert_eq!(active_name(&workspace, cx), "new 3");
    cx.simulate_keystrokes("ctrl-tab");
    assert_eq!(active_name(&workspace, cx), "new 1");
    release_ctrl(cx);
    cx.simulate_keystrokes("ctrl-shift-tab");
    assert_eq!(active_name(&workspace, cx), "new 3");
    release_ctrl(cx);

    // Off: Ctrl+Tab is Next Tab, without a list.
    tab_settings(&workspace, cx, |tabs| tabs.switcher = false);
    cx.simulate_keystrokes("ctrl-tab");
    assert_eq!(active_name(&workspace, cx), "new 1");
    assert!(!switcher_shown(cx));
    cx.simulate_keystrokes("ctrl-shift-tab");
    assert_eq!(active_name(&workspace, cx), "new 3");
}

#[gpui_kit::test]
fn the_switcher_with_one_document_or_one_closed(cx: &mut TestAppContext) {
    let (workspace, cx) = documents(1, cx);
    cx.simulate_keystrokes("ctrl-tab");
    assert_eq!(active_name(&workspace, cx), "new 1");
    release_ctrl(cx);
    // Releasing Ctrl without the switcher changes nothing.
    release_ctrl(cx);

    // A document closed while the list shows leaves it.
    cx.simulate_keystrokes(&secondary("n"));
    cx.simulate_keystrokes(&secondary("n"));
    cx.simulate_keystrokes("ctrl-tab");
    assert_eq!(active_name(&workspace, cx), "new 2");
    command(&workspace, "file.close", cx);
    assert_eq!(tab_names(&workspace, cx), ["new 1", "new 3"]);
    assert!(switcher_shown(cx));
    cx.simulate_keystrokes("ctrl-tab");
    release_ctrl(cx);
    assert_eq!(tab_names(&workspace, cx), ["new 1", "new 3"]);
    assert!(!switcher_shown(cx));
}

#[gpui_kit::test]
fn the_context_menu_acts_on_its_tab(cx: &mut TestAppContext) {
    let (workspace, cx) = documents(3, cx);
    let views = pane_views(&workspace, 0, cx);
    let pane = workspace.read_with(cx, |workspace, _| workspace.panes()[0].clone());
    let menu = |view: Option<Entity<birchpad::editor::EditorView>>,
                invocation: Invocation,
                cx: &mut VisualTestContext| {
        pane.update(cx, |_, cx| cx.emit(PaneEvent::Command { view, invocation }));
        cx.run_until_parked();
    };
    menu(Some(views[0].clone()), Invocation::new("view.pin-tab"), cx);
    assert_eq!(tabs(&workspace, cx), ["new 1*", "new 2", "new 3"]);
    assert_eq!(active_name(&workspace, cx), "new 1");
    menu(
        Some(views[1].clone()),
        Invocation::new("file.close-right"),
        cx,
    );
    assert_eq!(tabs(&workspace, cx), ["new 1*", "new 2"]);
    // The empty part of the bar has no document.
    menu(None, Invocation::new("file.new"), cx);
    assert_eq!(tab_names(&workspace, cx), ["new 1", "new 2", "new 3"]);
    // A failing command is reported, not panicking.
    menu(
        Some(views[1].clone()),
        Invocation::with_args("view.tab-color", json!({ "color": 9 })),
        cx,
    );
    assert_eq!(tabs(&workspace, cx), ["new 1*", "new 2", "new 3"]);
    assert_eq!(active_name(&workspace, cx), "new 2");

    // A right click opens the menu without changing the active tab.
    let tab = tab_bounds(0, cx).center();
    cx.simulate_mouse_down(tab, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(tab, MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(active_name(&workspace, cx), "new 2");
}

#[gpui_kit::test]
fn double_clicks_on_tabs_and_the_bar(cx: &mut TestAppContext) {
    let (workspace, cx) = documents(2, cx);
    // By default a double click on a tab only activates it.
    double_click(tab_bounds(0, cx).center(), cx);
    assert_eq!(tab_names(&workspace, cx), ["new 1", "new 2"]);
    assert_eq!(active_name(&workspace, cx), "new 1");

    tab_settings(&workspace, cx, |tabs| tabs.double_click_close = true);
    double_click(tab_bounds(1, cx).center(), cx);
    assert_eq!(tab_names(&workspace, cx), ["new 1"]);

    // On the empty part of the bar, a new document.
    let rest = bounds("pane-0-tab-bar-rest".into(), cx).expect("drawn");
    double_click(rest.center(), cx);
    assert_eq!(tab_names(&workspace, cx), ["new 1", "new 2"]);
}

#[gpui_kit::test]
fn the_close_button_can_be_hidden(cx: &mut TestAppContext) {
    let (workspace, cx) = documents(2, cx);
    assert!(bounds("pane-0-close-tab-0".into(), cx).is_some());
    tab_settings(&workspace, cx, |tabs| tabs.close_button = false);
    assert!(bounds("pane-0-close-tab-0".into(), cx).is_none());
    // The middle button still closes.
    click_on("pane-0-tab-0", MouseButton::Middle, cx);
    assert_eq!(tab_names(&workspace, cx), ["new 2"]);
}

#[gpui_kit::test]
fn locked_tabs_are_not_dragged(cx: &mut TestAppContext) {
    let (workspace, cx) = documents(3, cx);
    drag(tab_bounds(2, cx).center(), tab_bounds(0, cx).center(), cx);
    assert_eq!(tab_names(&workspace, cx), ["new 3", "new 1", "new 2"]);

    tab_settings(&workspace, cx, |tabs| tabs.lock = true);
    drag(tab_bounds(2, cx).center(), tab_bounds(0, cx).center(), cx);
    assert_eq!(tab_names(&workspace, cx), ["new 3", "new 1", "new 2"]);
}

#[gpui_kit::test]
fn the_bar_on_one_row_several_rows_or_a_column(cx: &mut TestAppContext) {
    let (workspace, cx) = documents(40, cx);
    // One row: every tab at the same height, the text below them.
    let [first, last] = [tab_bounds(0, cx), tab_bounds(39, cx)];
    assert_eq!(first.top(), last.top());
    let text = bounds("pane-content-0".into(), cx).unwrap();
    assert!(text.top() >= first.bottom());
    assert_eq!(first.size.height, gpui_kit::px(30.));

    // Several rows: the last tabs wrap below the first.
    tab_settings(&workspace, cx, |tabs| tabs.multi_line = true);
    assert!(tab_bounds(39, cx).top() > tab_bounds(0, cx).top());
    assert!(bounds("pane-content-0".into(), cx).unwrap().top() >= tab_bounds(39, cx).bottom());

    // A column: tabs one below the other, the text on their right.
    tab_settings(&workspace, cx, |tabs| tabs.vertical = true);
    let [first, second] = [tab_bounds(0, cx), tab_bounds(1, cx)];
    assert_eq!(first.left(), second.left());
    assert!(second.top() >= first.bottom());
    assert!(bounds("pane-content-0".into(), cx).unwrap().left() >= first.right());

    // Reduced: smaller tabs.
    tab_settings(&workspace, cx, |tabs| {
        tabs.vertical = false;
        tabs.multi_line = false;
        tabs.reduce = true;
    });
    assert_eq!(tab_bounds(0, cx).size.height, gpui_kit::px(24.));
}

#[gpui_kit::test]
fn tabs_are_kept_in_sessions(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let paths: Vec<_> = ["a.txt", "b.txt", "c.txt"]
        .iter()
        .map(|name| {
            let path = dir.path().join(name);
            fs::write(&path, *name).unwrap();
            path
        })
        .collect();
    let (workspace, cx) = open_workspace(cx);
    for path in &paths {
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_path(path, window, cx)
        });
        cx.run_until_parked();
    }
    go_to(&workspace, 1, cx);
    if active_name(&workspace, cx) == "new 1" {
        command(&workspace, "file.close", cx);
    }
    // c.txt pinned, b.txt colored.
    go_to(&workspace, 3, cx);
    command(&workspace, "view.pin-tab", cx);
    go_to(&workspace, 3, cx);
    run(
        &workspace,
        Invocation::with_args("view.tab-color", json!({ "color": 2 })),
        cx,
    )
    .unwrap();
    assert_eq!(tabs(&workspace, cx), ["c.txt*", "a.txt", "b.txt 2"]);
    let mut session = workspace.read_with(cx, |workspace, cx| workspace.session(false, cx));
    let marks: Vec<_> = session
        .main_view
        .tabs
        .iter()
        .map(|tab| (tab.pinned, tab.color))
        .collect();
    assert_eq!(marks, [(true, None), (false, None), (false, Some(2))]);

    // Restored, in another order and with a color past the last, which is dropped: the pinned
    // tab still goes first.
    session.main_view.tabs.reverse();
    session.main_view.tabs[2].color = Some(9);
    command(&workspace, "file.close-all", cx);
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.restore_session(&session, None, window, cx)
    });
    cx.run_until_parked();
    assert_eq!(
        tabs(&workspace, cx),
        ["c.txt*", "new 1", "b.txt 2", "a.txt"]
    );
}

#[test]
fn the_peeker_shows_the_start_of_the_text() {
    let text = Rope::from("one\n\ttwo\r\nthree");
    assert_eq!(peek_lines(&text, 0), ["one", "    two", "three"]);
    assert_eq!(peek_lines(&text, 1), ["    two", "three"]);
    // Past the end: the last line.
    assert_eq!(peek_lines(&text, 99), ["three"]);
    assert_eq!(peek_lines(&Rope::from(""), 0), [""]);
    assert_eq!(peek_lines(&Rope::from("a\n"), 0), ["a", ""]);

    // At most twelve lines, each cut at 72 characters; control characters as spaces.
    let many = (0..20)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let lines = peek_lines(&Rope::from(many.as_str()), 0);
    assert_eq!(lines.len(), birchpad::pane::PEEK_LINES);
    assert_eq!(lines.last().unwrap(), "11");
    let exact = "é".repeat(birchpad::pane::PEEK_COLUMNS);
    assert_eq!(
        peek_lines(&Rope::from(exact.as_str()), 0),
        std::slice::from_ref(&exact)
    );
    let long = format!("{exact}x");
    assert_eq!(
        peek_lines(&Rope::from(long.as_str()), 0),
        [format!("{exact}…")]
    );
    assert_eq!(peek_lines(&Rope::from("a\u{7}b"), 0), ["a b"]);
}
