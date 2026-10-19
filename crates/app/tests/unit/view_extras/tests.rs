use std::fs;

use anyhow::Result;
use birchpad_commands::Invocation;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use super::*;
use crate::file_commands::SHELL;
use crate::workspace::Workspace;
use crate::workspace::tests::open_workspace;

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

fn browser(name: &str) -> Invocation {
    Invocation::with_args("view.in-browser", json!({ "browser": name }))
}

fn shell_requests() -> Vec<Shell> {
    SHELL.with(|shell| std::mem::take(&mut *shell.borrow_mut()))
}

#[gpui_kit::test]
fn the_file_in_a_browser(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("my page.html");
    fs::write(&path, "<p>").unwrap();
    let (workspace, cx) = open_workspace(cx);
    // Untitled: no file to show.
    assert!(run(&workspace, browser("default"), cx).is_err());
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(&path, window, cx)
    });
    cx.run_until_parked();
    run(&workspace, browser("default"), cx).unwrap();
    run(&workspace, browser("firefox"), cx).unwrap();
    let requests = shell_requests();
    let [Shell::Browse(url), Shell::Browser(name, shown)] = requests.as_slice() else {
        panic!("{requests:?}");
    };
    assert!(
        url.starts_with("file:///") && !url.starts_with("file:////"),
        "{url}"
    );
    assert!(
        url.ends_with("my%20page.html") && !url.contains('\\'),
        "{url}"
    );
    assert_eq!(name, "firefox");
    assert!(shown.ends_with("my page.html"));
    // A browser it does not know, or none.
    assert!(run(&workspace, browser("lynx"), cx).is_err());
    assert!(run(&workspace, Invocation::new("view.in-browser"), cx).is_err());
    assert!(shell_requests().is_empty());
}

#[test]
fn the_summary_of_a_document() {
    crate::i18n::set_language("en");
    let counts = count_text(&"ab cd\nef".into());
    let rows = summary_rows(None, "new 1", counts, &[]);
    let values: Vec<&str> = rows.iter().map(|(_, value)| value.as_str()).collect();
    assert_eq!(values, ["new 1", "7", "3", "2", "8 bytes"]);
    // With a file: its times; with selections: their counts together.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, "x").unwrap();
    let selected = [count_text(&"ab".into()), count_text(&"c\nd".into())];
    let rows = summary_rows(Some(&path), "a.txt", counts, &selected);
    assert!(rows.iter().any(|(label, _)| label == "Modified"));
    let (_, last) = rows.last().unwrap();
    assert_eq!(last, "4 characters (5 bytes) in 3 lines, 2 selections");
}

#[gpui_kit::test]
fn the_summary_window(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("one two");
    run(&workspace, Invocation::new("view.summary"), cx).unwrap();
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("summary-0").is_some());
}
