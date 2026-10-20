use std::fs;
use std::path::Path;

use anyhow::Result;
use birchpad_commands::Invocation;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use crate::workspace::{active_text, document_start, open_workspace, secondary};
use birchpad::find::{FindBarEvent, FindTab};
use birchpad::search_extras::*;
use birchpad::workspace::Workspace;
use gpui_kit::AppContext as _;
use gpui_kit::component::WindowExt as _;

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

fn command(workspace: &Entity<Workspace>, id: &str, cx: &mut VisualTestContext) -> Result<()> {
    run(workspace, Invocation::new(id), cx)
}

fn open(workspace: &Entity<Workspace>, path: &Path, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(path, window, cx)
    });
    cx.run_until_parked();
}

fn selection(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> (usize, usize) {
    workspace.read_with(cx, |workspace, cx| {
        let range = workspace
            .active_view(cx)
            .unwrap()
            .read(cx)
            .selection
            .primary();
        (range.from(), range.to())
    })
}

fn caret_line(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> usize {
    workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap().read(cx);
        birchpad_core::motion::line_of(view.text(cx), view.selection.primary().head)
    })
}

#[gpui_kit::test]
fn going_through_changes_and_clearing_them(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, "0\n1\n2\n3\n4\n5\n").unwrap();
    let (workspace, cx) = open_workspace(cx);
    open(&workspace, &path, cx);
    // No change yet: nowhere to go.
    command(&workspace, "search.next-change", cx).unwrap();
    assert_eq!(caret_line(&workspace, cx), 0);

    // Lines 1 and 4 changed.
    cx.simulate_keystrokes("down end");
    cx.simulate_input("x");
    cx.simulate_keystrokes("down down down end");
    cx.simulate_input("y");
    cx.run_until_parked();
    cx.simulate_keystrokes(document_start());
    command(&workspace, "search.next-change", cx).unwrap();
    assert_eq!(caret_line(&workspace, cx), 1);
    command(&workspace, "search.next-change", cx).unwrap();
    assert_eq!(caret_line(&workspace, cx), 4);
    command(&workspace, "search.next-change", cx).unwrap();
    assert_eq!(caret_line(&workspace, cx), 1, "around");
    command(&workspace, "search.previous-change", cx).unwrap();
    assert_eq!(caret_line(&workspace, cx), 4, "around backwards");

    // Cleared: the text stays, there is no change to go to.
    command(&workspace, "search.clear-changes", cx).unwrap();
    cx.simulate_keystrokes(document_start());
    command(&workspace, "search.next-change", cx).unwrap();
    assert_eq!(caret_line(&workspace, cx), 0);
    assert_eq!(active_text(&workspace, cx), "0\n1x\n2\n3\n4y\n5\n");
}

fn in_range(from: u32, to: u32, backward: bool, wrap: bool) -> Invocation {
    Invocation::with_args(
        "search.find-in-range",
        json!({ "from": from, "to": to, "backward": backward, "wrap": wrap }),
    )
}

#[gpui_kit::test]
fn finding_characters_in_a_range(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("aé\tb€");
    cx.simulate_keystrokes(document_start());
    run(&workspace, in_range(128, MAX_CODE, false, false), cx).unwrap();
    assert_eq!(selection(&workspace, cx), (1, 3));
    run(&workspace, in_range(128, MAX_CODE, false, false), cx).unwrap();
    assert_eq!(selection(&workspace, cx), (5, 8));
    // At the end, without wrapping: not found, the selection stays.
    let error = run(&workspace, in_range(128, MAX_CODE, false, false), cx).unwrap_err();
    assert!(error.to_string().contains("range"), "{error}");
    assert_eq!(selection(&workspace, cx), (5, 8));
    run(&workspace, in_range(0, 31, true, true), cx).unwrap();
    assert_eq!(selection(&workspace, cx), (3, 4));
    // Not a range, or missing arguments.
    assert!(run(&workspace, in_range(9, 1, false, true), cx).is_err());
    assert!(run(&workspace, in_range(0, MAX_CODE + 1, false, true), cx).is_err());
    assert!(command(&workspace, "search.find-in-range", cx).is_err());
}

#[test]
fn character_codes_as_typed() {
    assert_eq!(parse_code("65"), Some(65));
    assert_eq!(parse_code(" 0x41 "), Some(65));
    assert_eq!(parse_code("U+20AC"), Some(0x20AC));
    assert_eq!(parse_code("u+10ffff"), Some(MAX_CODE));
    assert_eq!(parse_code("0"), Some(0));
    for bad in ["", "-1", "0x", "0x110000", "1114112", "abc", "0xg"] {
        assert_eq!(parse_code(bad), None, "{bad}");
    }
}

#[gpui_kit::test]
fn the_find_characters_in_range_window(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("x7é");
    cx.simulate_keystrokes(document_start());
    command(&workspace, "search.find-characters-in-range", cx).unwrap();
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    let weak = workspace.downgrade();
    let dialog = workspace.update_in(cx, |_, window, cx| {
        cx.new(|cx| RangeDialog::new(weak, window, cx))
    });
    let find = |cx: &mut VisualTestContext| {
        dialog.update_in(cx, |dialog, window, cx| dialog.find(window, cx));
        cx.run_until_parked();
        dialog.read_with(cx, |dialog, _| dialog.error.clone())
    };
    // Non-ASCII by default.
    assert_eq!(find(cx), None);
    assert_eq!(selection(&workspace, cx), (2, 4));
    // ASCII, backwards: the 7.
    dialog.update(cx, |dialog, _| {
        dialog.range = Range::Ascii;
        dialog.backward = true;
    });
    assert_eq!(find(cx), None);
    assert_eq!(selection(&workspace, cx), (1, 2));
    // One's own range, as typed; nothing in it, or not a range: said in the window.
    dialog.update_in(cx, |dialog, window, cx| {
        dialog.range = Range::Own;
        dialog.backward = false;
        dialog.wrap = false;
        dialog
            .from
            .update(cx, |input, cx| input.set_value("0x30", window, cx));
        dialog
            .to
            .update(cx, |input, cx| input.set_value("57", window, cx));
    });
    assert!(
        find(cx).is_some(),
        "past the 7, nothing more without wrapping"
    );
    dialog.update(cx, |dialog, _| dialog.wrap = true);
    assert_eq!(find(cx), None);
    assert_eq!(selection(&workspace, cx), (1, 2));
    dialog.update_in(cx, |dialog, window, cx| {
        dialog
            .to
            .update(cx, |input, cx| input.set_value("seven", window, cx));
    });
    assert!(find(cx).unwrap().contains("seven"));
}

fn set_find_text(
    workspace: &Entity<Workspace>,
    text: &str,
    tab: FindTab,
    cx: &mut VisualTestContext,
) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.find_bar.update(cx, |bar, cx| {
            bar.show(tab, Some(text.to_owned()), None, window, cx);
        });
    });
    cx.run_until_parked();
}

fn find_text(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    workspace.read_with(cx, |workspace, cx| {
        workspace
            .find_bar
            .read(cx)
            .find_input
            .read(cx)
            .value()
            .to_string()
    })
}

#[gpui_kit::test]
fn volatile_find_leaves_the_find_text(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("foo bar foo food");
    set_find_text(&workspace, "bar", FindTab::Find, cx);
    // Back in the text.
    workspace.update_in(cx, |workspace, window, cx| {
        let view = workspace.active_view(cx).unwrap();
        workspace.activate_view(&view, window, cx);
    });
    cx.simulate_keystrokes(document_start());
    // The word at the caret, whole: "food" is not found.
    command(&workspace, "search.find-volatile-next", cx).unwrap();
    assert_eq!(selection(&workspace, cx), (8, 11));
    // Now the selection is searched as it is: inside "food" too.
    command(&workspace, "search.find-volatile-next", cx).unwrap();
    assert_eq!(selection(&workspace, cx), (12, 15));
    command(&workspace, "search.find-volatile-next", cx).unwrap();
    assert_eq!(selection(&workspace, cx), (0, 3), "around");
    command(&workspace, "search.find-volatile-previous", cx).unwrap();
    assert_eq!(selection(&workspace, cx), (12, 15), "around backwards");
    assert_eq!(find_text(&workspace, cx), "bar");
    // Select and Find takes the text into the panel.
    command(&workspace, "search.select-and-find-next", cx).unwrap();
    assert_eq!(find_text(&workspace, cx), "foo");
    // Between two spaces, or in an empty document: nothing to look for.
    cx.simulate_keystrokes(&secondary("a"));
    cx.simulate_input("a  b");
    cx.simulate_keystrokes("left left");
    command(&workspace, "search.find-volatile-next", cx).unwrap();
    assert_eq!(selection(&workspace, cx), (2, 2));
    cx.simulate_keystrokes(&secondary("a"));
    cx.simulate_keystrokes("backspace");
    command(&workspace, "search.find-volatile-previous", cx).unwrap();
    assert_eq!(selection(&workspace, cx), (0, 0));
}

fn rows(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    workspace.read_with(cx, |workspace, cx| {
        workspace.search_results.read(cx).rows_text()
    })
}

fn status(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    workspace.read_with(cx, |workspace, cx| {
        workspace.find_bar.read(cx).status_text()
    })
}

fn find_in_projects(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        let bar = workspace.find_bar.clone();
        workspace.on_find_bar_event(&bar, &FindBarEvent::FindInProjects, window, cx);
    });
    cx.run_until_parked();
}

/// A Notepad++ workspace file `name` in `dir` with one project of `files`.
fn workspace_file(dir: &Path, name: &str, files: &[&Path]) -> std::path::PathBuf {
    let items: String = files
        .iter()
        .map(|file| format!("<File name=\"{}\" />", file.display()))
        .collect();
    let path = dir.join(name);
    fs::write(
        &path,
        format!("<NotepadPlus><Project name=\"P\">{items}</Project></NotepadPlus>"),
    )
    .unwrap();
    path
}

fn set_projects(cx: &mut VisualTestContext, panels: &[(&str, std::path::PathBuf)]) {
    cx.update(|_, cx| {
        let projects = &mut cx
            .global_mut::<birchpad::app_state::AppState>()
            .state
            .panels
            .projects;
        for (panel, file) in panels {
            projects.insert((*panel).to_owned(), file.clone());
        }
    });
}

#[gpui_kit::test]
fn finding_in_the_files_of_projects(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let [a, b, c] = ["a.txt", "b.txt", "c.txt"].map(|name| {
        let path = dir.path().join(name);
        fs::write(&path, format!("needle in {name}\n")).unwrap();
        path
    });
    let one = workspace_file(dir.path(), "one.workspace", &[&a, &b]);
    let three = workspace_file(dir.path(), "three.workspace", &[&b, &c]);
    let (workspace, cx) = open_workspace(cx);
    set_projects(cx, &[("1", one), ("3", three)]);
    set_find_text(&workspace, "needle", FindTab::FindInProjects, cx);
    find_in_projects(&workspace, cx);
    let found = rows(&workspace, cx);
    let files = found.iter().filter(|row| row.ends_with("(1)")).count();
    assert_eq!(files, 3, "b.txt once: {found:#?}");

    // Only Project Panel 1: the newest search has two files.
    workspace.update(cx, |workspace, cx| {
        workspace
            .find_bar
            .update(cx, |bar, _| bar.project_panels = [true, false, false]);
    });
    find_in_projects(&workspace, cx);
    let newest: Vec<String> = rows(&workspace, cx)
        .into_iter()
        .skip(1)
        .take_while(|row| row.starts_with(' '))
        .filter(|row| row.ends_with("(1)"))
        .collect();
    assert_eq!(newest.len(), 2, "{newest:#?}");
    assert!(newest.iter().all(|row| !row.contains("c.txt")));

    // No panel with files, or a workspace that cannot be read: said in the panel.
    workspace.update(cx, |workspace, cx| {
        workspace
            .find_bar
            .update(cx, |bar, _| bar.project_panels = [false, true, false]);
    });
    find_in_projects(&workspace, cx);
    assert!(status(&workspace, cx).unwrap().contains("no files"));
    set_projects(cx, &[("2", dir.path().join("missing.workspace"))]);
    find_in_projects(&workspace, cx);
    assert!(
        status(&workspace, cx)
            .unwrap()
            .contains("missing.workspace")
    );
}

#[gpui_kit::test]
fn the_results_purge_wrap_and_open_documents(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.txt");
    fs::write(&a, "x needle\n").unwrap();
    let project = workspace_file(dir.path(), "p.workspace", &[&a]);
    let (workspace, cx) = open_workspace(cx);
    set_projects(cx, &[("1", project)]);
    set_find_text(&workspace, "needle", FindTab::FindInProjects, cx);
    find_in_projects(&workspace, cx);
    find_in_projects(&workspace, cx);
    let searches = |cx: &mut VisualTestContext| {
        rows(&workspace, cx)
            .iter()
            .filter(|row| !row.starts_with(' '))
            .count()
    };
    assert_eq!(searches(cx), 2);

    // Purge for Every Search: one search at a time; both switches remembered.
    let results = workspace.read_with(cx, |workspace, _| workspace.search_results.clone());
    results.update(cx, |results, cx| results.set_switches(true, true, cx));
    find_in_projects(&workspace, cx);
    assert_eq!(searches(cx), 1);
    let remembered = cx.update(|_, cx| {
        birchpad::app_state::AppState::global(cx)
            .state
            .search_results
            .clone()
    });
    assert!(remembered.wrap_long_lines && remembered.purge);
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("search-result-0").is_some(),
        "drawn while wrapping"
    );

    // Open Selected Pathname, on the document's row; the search's row has none.
    let tabs = |cx: &mut VisualTestContext| crate::workspace::tab_names(&workspace, cx);
    results.update(cx, |results, cx| results.open_documents(0, false, cx));
    cx.run_until_parked();
    assert_eq!(tabs(cx), ["new 1"]);
    results.update(cx, |results, cx| results.open_documents(1, false, cx));
    cx.run_until_parked();
    assert_eq!(tabs(cx), ["a.txt"]);
    command(&workspace, "file.close-all", cx).unwrap();
    results.update(cx, |results, cx| results.open_documents(0, true, cx));
    cx.run_until_parked();
    assert_eq!(tabs(cx), ["a.txt"]);
    // A row past the end does nothing.
    results.update(cx, |results, cx| results.open_documents(99, true, cx));
}
