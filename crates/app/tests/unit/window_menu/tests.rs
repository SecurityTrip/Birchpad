use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::Result;
use birchpad_commands::Invocation;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use super::*;
use crate::workspace::tests::{open_workspace, secondary, tab_names};

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

fn sort_tabs(
    workspace: &Entity<Workspace>,
    by: &str,
    descending: bool,
    cx: &mut VisualTestContext,
) -> Vec<String> {
    run(
        workspace,
        Invocation::with_args("window.sort", json!({ "by": by, "descending": descending })),
        cx,
    )
    .unwrap();
    tab_names(workspace, cx)
}

/// Writes `text` to `name` in `dir`, last changed `age` seconds before a fixed time.
fn file(dir: &Path, name: &str, text: &str, age: u64) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, text).unwrap();
    let time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000 - age);
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(time)
        .unwrap();
    path
}

fn open(workspace: &Entity<Workspace>, path: &Path, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(path, window, cx)
    });
    cx.run_until_parked();
}

fn active_name(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.read(cx).buffer.read(cx).display_name()
    })
}

/// Three files in a fresh window, "new 1" closed: b.txt (5 bytes, oldest), A.txt (1 byte,
/// newest) and c.rs (3 bytes), in that order.
fn three_files(
    cx: &mut TestAppContext,
) -> (Entity<Workspace>, &mut VisualTestContext, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let (workspace, cx) = open_workspace(cx);
    for (name, text, age) in [
        ("b.txt", "bbbbb", 30),
        ("A.txt", "a", 10),
        ("c.rs", "ccc", 20),
    ] {
        open(&workspace, &file(dir.path(), name, text, age), cx);
    }
    run(
        &workspace,
        Invocation::with_args("view.go-to-tab", json!({ "index": 1 })),
        cx,
    )
    .unwrap();
    if active_name(&workspace, cx) == "new 1" {
        run(&workspace, Invocation::new("file.close"), cx).unwrap();
    }
    assert_eq!(tab_names(&workspace, cx), ["b.txt", "A.txt", "c.rs"]);
    (workspace, cx, dir)
}

#[gpui_kit::test]
fn sort_by_orders_the_tabs(cx: &mut TestAppContext) {
    let (workspace, cx, _dir) = three_files(cx);
    // Names without regard to case.
    assert_eq!(
        sort_tabs(&workspace, "name", false, cx),
        ["A.txt", "b.txt", "c.rs"]
    );
    assert_eq!(
        sort_tabs(&workspace, "name", true, cx),
        ["c.rs", "b.txt", "A.txt"]
    );
    assert_eq!(
        sort_tabs(&workspace, "path", false, cx),
        ["A.txt", "b.txt", "c.rs"]
    );
    // Rust before Normal text, by the language's name.
    assert_eq!(sort_tabs(&workspace, "type", true, cx)[0], "c.rs");
    assert_eq!(
        sort_tabs(&workspace, "size", false, cx),
        ["A.txt", "c.rs", "b.txt"]
    );
    assert_eq!(
        sort_tabs(&workspace, "size", true, cx),
        ["b.txt", "c.rs", "A.txt"]
    );
    assert_eq!(
        sort_tabs(&workspace, "modified", false, cx),
        ["b.txt", "c.rs", "A.txt"]
    );
    assert_eq!(
        sort_tabs(&workspace, "modified", true, cx),
        ["A.txt", "c.rs", "b.txt"]
    );
    // The active document stays active.
    assert_eq!(active_name(&workspace, cx), "b.txt");
}

#[gpui_kit::test]
fn sort_by_keeps_pinned_tabs_first_and_equal_ones_in_order(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_keystrokes(&secondary("n"));
    cx.simulate_keystrokes(&secondary("n"));
    // Untitled documents have no time and the same size: their order stays.
    assert_eq!(
        sort_tabs(&workspace, "modified", false, cx),
        ["new 1", "new 2", "new 3"]
    );
    assert_eq!(
        sort_tabs(&workspace, "size", true, cx),
        ["new 1", "new 2", "new 3"]
    );
    run(&workspace, Invocation::new("view.pin-tab"), cx).unwrap();
    assert_eq!(
        sort_tabs(&workspace, "name", false, cx),
        ["new 3", "new 1", "new 2"]
    );
    assert_eq!(
        sort_tabs(&workspace, "name", true, cx),
        ["new 3", "new 2", "new 1"]
    );
}

#[gpui_kit::test]
fn sort_by_refuses_what_it_does_not_know(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_keystrokes(&secondary("n"));
    for args in [json!({ "by": "colour" }), json!({}), json!({ "by": 3 })] {
        assert!(
            run(
                &workspace,
                Invocation::with_args("window.sort", args.clone()),
                cx
            )
            .is_err(),
            "{args}"
        );
    }
    assert!(run(&workspace, Invocation::new("window.sort"), cx).is_err());
    assert_eq!(tab_names(&workspace, cx), ["new 1", "new 2"]);
    // Without "descending", ascending.
    run(
        &workspace,
        Invocation::with_args("window.sort", json!({ "by": "name" })),
        cx,
    )
    .unwrap();
    assert_eq!(tab_names(&workspace, cx), ["new 1", "new 2"]);
}

#[gpui_kit::test]
fn the_window_menu_lists_the_active_views_documents(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_keystrokes(&secondary("n"));
    let documents = |cx: &mut VisualTestContext| workspace.read_with(cx, menu_documents);
    assert_eq!(
        documents(cx),
        (vec!["new 1".into(), "new 2".into()], Some(1))
    );
    // The second view's documents when it is the active one.
    run(&workspace, Invocation::new("view.move-to-other-view"), cx).unwrap();
    assert_eq!(documents(cx), (vec!["new 2".into()], Some(0)));
    // An item of the list is Go to Tab.
    run(&workspace, Invocation::new("view.focus-other-view"), cx).unwrap();
    cx.simulate_keystrokes(&secondary("n"));
    run(
        &workspace,
        Invocation::with_args("view.go-to-tab", json!({ "index": 1 })),
        cx,
    )
    .unwrap();
    assert_eq!(
        documents(cx),
        (vec!["new 1".into(), "new 3".into()], Some(0))
    );
}

fn dialog(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<WindowsDialog> {
    let weak = workspace.downgrade();
    let active = workspace.read_with(cx, |workspace, cx| workspace.active_view(cx));
    cx.new(|_| WindowsDialog::new(weak, active.as_ref()))
}

fn rows(dialog: &Entity<WindowsDialog>, cx: &mut VisualTestContext) -> Vec<String> {
    dialog.read_with(cx, |dialog, cx| {
        dialog.rows(cx).into_iter().map(|row| row.name).collect()
    })
}

fn selection(dialog: &Entity<WindowsDialog>, cx: &mut VisualTestContext) -> Vec<String> {
    dialog.read_with(cx, |dialog, cx| {
        dialog
            .selection(cx)
            .iter()
            .map(|view| view.read(cx).buffer.read(cx).display_name())
            .collect()
    })
}

#[gpui_kit::test]
fn the_dialog_sorts_and_selects(cx: &mut TestAppContext) {
    let (workspace, cx, _dir) = three_files(cx);
    run(&workspace, Invocation::new("view.last-tab"), cx).unwrap();
    let dialog = dialog(&workspace, cx);
    // The tabs' order, the active document selected.
    assert_eq!(rows(&dialog, cx), ["b.txt", "A.txt", "c.rs"]);
    assert_eq!(selection(&dialog, cx), ["c.rs"]);

    // A heading sorts, in reverse on a second click; another heading starts ascending.
    dialog.update(cx, |dialog, cx| dialog.sort_by(SortBy::Name, cx));
    assert_eq!(rows(&dialog, cx), ["A.txt", "b.txt", "c.rs"]);
    dialog.update(cx, |dialog, cx| dialog.sort_by(SortBy::Name, cx));
    assert_eq!(rows(&dialog, cx), ["c.rs", "b.txt", "A.txt"]);
    dialog.update(cx, |dialog, cx| dialog.sort_by(SortBy::Size, cx));
    assert_eq!(rows(&dialog, cx), ["A.txt", "c.rs", "b.txt"]);

    // Clicks: alone, Ctrl adds and removes, Shift selects from the last one clicked.
    dialog.update(cx, |dialog, cx| dialog.click(0, false, false, cx));
    assert_eq!(selection(&dialog, cx), ["A.txt"]);
    dialog.update(cx, |dialog, cx| dialog.click(2, true, false, cx));
    assert_eq!(selection(&dialog, cx), ["A.txt", "b.txt"]);
    dialog.update(cx, |dialog, cx| dialog.click(0, true, false, cx));
    assert_eq!(selection(&dialog, cx), ["b.txt"]);
    dialog.update(cx, |dialog, cx| dialog.click(1, false, true, cx));
    assert_eq!(
        selection(&dialog, cx),
        ["A.txt", "c.rs"],
        "from the last row clicked"
    );
    // A row past the end changes nothing.
    dialog.update(cx, |dialog, cx| dialog.click(3, false, false, cx));
    assert_eq!(selection(&dialog, cx), ["A.txt", "c.rs"]);

    // Sort Tabs: the tabs take the order shown, which is now the tabs' own.
    dialog.update(cx, |dialog, cx| dialog.sort_tabs(cx));
    assert_eq!(tab_names(&workspace, cx), ["A.txt", "c.rs", "b.txt"]);
    assert!(dialog.read_with(cx, |dialog, _| dialog.sort.is_none()));
    assert_eq!(rows(&dialog, cx), ["A.txt", "c.rs", "b.txt"]);
}

#[gpui_kit::test]
fn the_dialog_activates_saves_and_closes(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = three_files(cx);
    cx.simulate_input("x");
    assert_eq!(active_name(&workspace, cx), "b.txt");
    run(&workspace, Invocation::new("window.windows"), cx).unwrap();
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    let dialog = dialog(&workspace, cx);

    // Activate needs one document selected.
    dialog.update(cx, |dialog, cx| dialog.click(0, false, false, cx));
    dialog.update(cx, |dialog, cx| dialog.click(2, true, false, cx));
    dialog.update_in(cx, |dialog, window, cx| dialog.activate(window, cx));
    cx.run_until_parked();
    assert_eq!(active_name(&workspace, cx), "b.txt");
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));

    // Save: the selected documents with changes are written.
    dialog.update_in(cx, |dialog, window, cx| dialog.save(window, cx));
    cx.run_until_parked();
    assert_eq!(
        fs::read_to_string(dir.path().join("b.txt")).unwrap(),
        "xbbbbb"
    );

    // Close Window(s): the selected ones; what closed leaves the selection.
    dialog.update_in(cx, |dialog, window, cx| dialog.close(window, cx));
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx), ["A.txt"]);
    assert_eq!(selection(&dialog, cx), Vec::<String>::new());
    // Nothing selected: Activate, Save and Close do nothing.
    dialog.update_in(cx, |dialog, window, cx| {
        dialog.activate(window, cx);
        dialog.save(window, cx);
        dialog.close(window, cx);
    });
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx), ["A.txt"]);

    // One selected: it is shown and the dialog closes.
    cx.simulate_keystrokes(&secondary("n"));
    dialog.update(cx, |dialog, cx| dialog.click(0, false, false, cx));
    dialog.update_in(cx, |dialog, window, cx| dialog.activate(window, cx));
    cx.run_until_parked();
    assert_eq!(active_name(&workspace, cx), "A.txt");
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
}

#[gpui_kit::test]
fn closing_from_the_dialog_asks_about_changes(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("draft");
    cx.simulate_keystrokes(&secondary("n"));
    let dialog = dialog(&workspace, cx);
    dialog.update(cx, |dialog, cx| dialog.click(0, false, true, cx));
    assert_eq!(selection(&dialog, cx), ["new 1", "new 2"]);
    dialog.update_in(cx, |dialog, window, cx| dialog.close(window, cx));
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx), ["new 1", "new 2"]);
    assert_eq!(selection(&dialog, cx), ["new 1", "new 2"]);
}

#[gpui_kit::test]
fn the_dialog_by_mouse(cx: &mut TestAppContext) {
    use gpui_kit::{Modifiers, MouseButton, MouseDownEvent, MouseUpEvent};
    let (workspace, cx, _dir) = three_files(cx);
    run(&workspace, Invocation::new("window.windows"), cx).unwrap();
    // The Name heading sorts A.txt first; a double click on it shows it and closes the dialog.
    crate::workspace::tests::click_on("windows-heading-window-menu.name", MouseButton::Left, cx);
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let row = cx.debug_bounds("windows-row-0").expect("drawn").center();
    for click_count in [1, 2] {
        cx.simulate_event(MouseDownEvent {
            position: row,
            modifiers: Modifiers::none(),
            button: MouseButton::Left,
            click_count,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position: row,
            modifiers: Modifiers::none(),
            button: MouseButton::Left,
            click_count,
        });
    }
    cx.run_until_parked();
    assert_eq!(active_name(&workspace, cx), "A.txt");
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
    assert_eq!(
        tab_names(&workspace, cx),
        ["b.txt", "A.txt", "c.rs"],
        "tabs not sorted"
    );
}
