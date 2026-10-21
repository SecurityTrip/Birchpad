use std::fs;
use std::path::Path;

use gpui_kit::{Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext, point, px};
use serde_json::json;

use crate::workspace::{active_text, open_workspace, pane_views, secondary, tab_names};
use birchpad::app_state::AppState;
use birchpad::context_menu::EditorEvent;
use birchpad::context_menu::*;
use birchpad::workspace::Workspace;
use birchpad_commands::Invocation;

fn with_settings_folder(cx: &mut VisualTestContext, dir: &Path) {
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().paths.user_settings = Some(dir.join("settings.toml"));
    });
}

#[gpui_kit::test]
fn the_menu_comes_from_the_file_or_the_default(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (_, cx) = open_workspace(cx);
    let load = |cx: &mut VisualTestContext| cx.update(|_, cx| load(cx));
    let default = parse(DEFAULT).unwrap();
    // No settings folder, or no file: the default.
    assert_eq!(load(cx), (default.clone(), None));
    with_settings_folder(cx, dir.path());
    assert_eq!(load(cx), (default.clone(), None));

    let file = dir.path().join(FILE_NAME);
    fs::write(&file, "[[item]]\ncommand = \"edit.undo\"\n").unwrap();
    let (nodes, problem) = load(cx);
    assert_eq!(
        nodes,
        [Node::Command {
            invocation: Invocation::new("edit.undo"),
            label: None
        }]
    );
    assert_eq!(problem, None);

    // A broken file: the default, and why.
    fs::write(&file, "[[item]\n").unwrap();
    let (nodes, problem) = load(cx);
    assert_eq!(nodes, default);
    assert!(problem.unwrap().contains(FILE_NAME));
}

#[gpui_kit::test]
fn edit_popup_context_menu_opens_the_file(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (workspace, cx) = open_workspace(cx);
    let run = |workspace: &Entity<Workspace>, cx: &mut VisualTestContext| {
        let result = workspace.update_in(cx, |workspace, window, cx| {
            workspace.dispatch(&Invocation::new("settings.edit-context-menu"), window, cx)
        });
        cx.run_until_parked();
        result
    };
    assert!(run(&workspace, cx).is_err(), "no settings folder");

    let folder = dir.path().join("Birchpad");
    with_settings_folder(cx, &folder);
    run(&workspace, cx).unwrap();
    let file = folder.join(FILE_NAME);
    assert_eq!(fs::read_to_string(&file).unwrap(), DEFAULT);
    assert_eq!(tab_names(&workspace, cx), [FILE_NAME]);

    // An existing file is opened as it is.
    fs::write(&file, "# mine\n").unwrap();
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.dispatch(&Invocation::new("file.close-all"), window, cx)
        })
        .unwrap();
    cx.run_until_parked();
    run(&workspace, cx).unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), "# mine\n");
    assert_eq!(active_text(&workspace, cx), "# mine\n");
}

#[gpui_kit::test]
fn a_chosen_item_runs_on_its_document(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("first");
    cx.simulate_keystrokes(&secondary("n"));
    let first = pane_views(&workspace, 0, cx)[0].clone();
    first.update(cx, |_, cx| {
        cx.emit(EditorEvent::Command(Invocation::with_args(
            "edit.insert-text",
            json!({ "text": "!" }),
        )))
    });
    cx.run_until_parked();
    assert_eq!(active_text(&workspace, cx), "first!");
    // A command that fails is reported, nothing else happens.
    first.update(cx, |_, cx| {
        cx.emit(EditorEvent::Command(Invocation::new("view.go-to-tab")))
    });
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx), ["new 1", "new 2"]);
}

#[gpui_kit::test]
fn a_right_click_places_the_caret_outside_the_selection(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("abc def");
    let view = pane_views(&workspace, 0, cx)[0].clone();
    let bounds = view.read_with(cx, |view, _| view.bounds().expect("drawn"));
    let right_click = |at: gpui_kit::Point<gpui_kit::Pixels>, cx: &mut VisualTestContext| {
        cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::none());
        cx.simulate_mouse_up(at, MouseButton::Right, Modifiers::none());
        cx.run_until_parked();
        cx.simulate_keystrokes("escape");
    };
    let line = point(bounds.right() - px(40.), bounds.top() + px(8.));
    let selection = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            let range = view.selection.primary();
            (range.from(), range.to())
        })
    };
    // In the selection: it stays, for the menu to act on.
    cx.simulate_keystrokes(&secondary("a"));
    right_click(line, cx);
    assert_eq!(selection(cx), (0, 7));
    // Elsewhere: the caret goes there (past the end of the line: its end).
    cx.simulate_keystrokes("home shift-right");
    right_click(line, cx);
    assert_eq!(selection(cx), (7, 7));
}
