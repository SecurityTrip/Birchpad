use std::fs;
use std::path::Path;

use anyhow::Result;
use birchpad_commands::Invocation;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use crate::workspace::{active_text, open_workspace, secondary, tab_names};
use birchpad::app_state::AppState;
use birchpad::edit_extras::*;
use birchpad::file_commands::{SHELL, Shell};
use birchpad::workspace::Workspace;
use gpui_kit::ClipboardItem;

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

fn select_all(cx: &mut VisualTestContext) {
    cx.simulate_keystrokes(&secondary("a"));
}

fn read_only(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> Option<birchpad::buffer::ReadOnly> {
    workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.read(cx).buffer.read(cx).read_only()
    })
}

fn open(workspace: &Entity<Workspace>, path: &Path, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(path, window, cx)
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn set_read_only_stops_editing(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("a");
    command(&workspace, "edit.toggle-read-only", cx).unwrap();
    assert_eq!(
        read_only(&workspace, cx),
        Some(birchpad::buffer::ReadOnly::Requested)
    );
    cx.simulate_input("b");
    assert_eq!(active_text(&workspace, cx), "a");
    command(&workspace, "edit.toggle-read-only", cx).unwrap();
    assert_eq!(read_only(&workspace, cx), None);
    cx.simulate_input("b");
    assert_eq!(active_text(&workspace, cx), "ab");
    // An untitled document has no flag to clear.
    assert!(command(&workspace, "edit.clear-read-only-flag", cx).is_err());
}

#[gpui_kit::test]
fn clearing_the_read_only_flag_of_a_file(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("locked.txt");
    fs::write(&path, "x").unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).unwrap();
    let (workspace, cx) = open_workspace(cx);
    open(&workspace, &path, cx);
    assert_eq!(
        read_only(&workspace, cx),
        Some(birchpad::buffer::ReadOnly::File)
    );

    command(&workspace, "edit.clear-read-only-flag", cx).unwrap();
    assert!(!fs::metadata(&path).unwrap().permissions().readonly());
    assert_eq!(read_only(&workspace, cx), None);
    cx.simulate_input("y");
    cx.simulate_keystrokes(&secondary("s"));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&path).unwrap(), "yx");
    // Not read-only any more: an error, nothing changes.
    let error = command(&workspace, "edit.clear-read-only-flag", cx).unwrap_err();
    assert!(error.to_string().contains("not read-only"), "{error}");
}

#[gpui_kit::test]
fn binary_content_keeps_nul_characters(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    run(
        &workspace,
        Invocation::with_args("edit.insert-text", json!({ "text": "a\0b" })),
        cx,
    )
    .unwrap();
    select_all(cx);
    command(&workspace, "edit.copy-binary", cx).unwrap();
    // Other programs get the text without the NUL.
    let plain = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(plain.as_deref(), Some("ab"));
    command(&workspace, "edit.cut-binary", cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "");
    command(&workspace, "edit.paste-binary", cx).unwrap();
    command(&workspace, "edit.paste-binary", cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "a\0ba\0b");

    // Plain text from elsewhere pastes as it is; nothing selected copies nothing.
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("plain".into())));
    command(&workspace, "edit.paste-binary", cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "a\0ba\0bplain");
    command(&workspace, "edit.copy-binary", cx).unwrap();
    let after = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(after.as_deref(), Some("plain"));
}

#[gpui_kit::test]
fn pasting_html_and_rtf(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    rich_clipboard::FAKE.with(|fake| fake.borrow_mut().clear());
    let error = command(&workspace, "edit.paste-html", cx).unwrap_err();
    assert!(error.to_string().contains("HTML"), "{error}");
    assert!(command(&workspace, "edit.paste-rtf", cx).is_err());
    rich_clipboard::FAKE.with(|fake| {
        let mut fake = fake.borrow_mut();
        fake.push((RichFormat::Html, "<b>x</b>".into()));
        fake.push((RichFormat::Rtf, "{\\rtf1 x}".into()));
    });
    command(&workspace, "edit.paste-html", cx).unwrap();
    command(&workspace, "edit.paste-rtf", cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "<b>x</b>{\\rtf1 x}");
}

#[test]
fn the_html_of_windows_html_format() {
    use rich_clipboard::html_of_windows_format;
    let data = "Version:0.9\r\nStartHTML:55\r\nEndHTML:70\r\n\r\n\r\n\r\n<html>x</html>";
    let start = data.find("<html>").unwrap();
    let data = data.replace("StartHTML:55", &format!("StartHTML:{start:02}"));
    assert_eq!(html_of_windows_format(&data), "<html>x</html>");
    // Without a start, or one past the end: everything.
    assert_eq!(html_of_windows_format("<p>y</p>"), "<p>y</p>");
    assert_eq!(
        html_of_windows_format("StartHTML:999\r\n<p>"),
        "StartHTML:999\r\n<p>"
    );
    assert_eq!(html_of_windows_format(""), "");
}

fn shell_requests() -> Vec<Shell> {
    SHELL.with(|shell| std::mem::take(&mut *shell.borrow_mut()))
}

#[gpui_kit::test]
fn on_selection_opens_shows_and_searches(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let notes = dir.path().join("notes.txt");
    fs::write(&notes, "other.txt").unwrap();
    fs::write(dir.path().join("other.txt"), "other").unwrap();
    let (workspace, cx) = open_workspace(cx);

    // Nothing selected.
    for id in [
        "edit.open-selected-file",
        "edit.open-selected-folder",
        "edit.search-online",
    ] {
        let error = command(&workspace, id, cx).unwrap_err();
        assert!(error.to_string().contains("select"), "{id}: {error}");
    }

    // A name next to the document.
    open(&workspace, &notes, cx);
    select_all(cx);
    command(&workspace, "edit.open-selected-file", cx).unwrap();
    assert_eq!(tab_names(&workspace, cx), ["notes.txt", "other.txt"]);

    // Open Containing Folder shows it; a name that is not there is an error.
    select_all(cx);
    command(&workspace, "edit.open-selected-folder", cx).unwrap_err();
    run(
        &workspace,
        Invocation::with_args("edit.insert-text", json!({ "text": "  \"other.txt\" " })),
        cx,
    )
    .unwrap();
    select_all(cx);
    command(&workspace, "edit.open-selected-folder", cx).unwrap();
    assert!(matches!(&shell_requests()[..], [Shell::Reveal(path)] if path.ends_with("other.txt")));
    select_all(cx);
    run(
        &workspace,
        Invocation::with_args("edit.insert-text", json!({ "text": "missing.txt" })),
        cx,
    )
    .unwrap();
    select_all(cx);
    let error = command(&workspace, "edit.open-selected-file", cx).unwrap_err();
    assert!(error.to_string().contains("missing.txt"), "{error}");

    // Search on Internet with the chosen engine; a custom address must be a web address.
    command(&workspace, "edit.search-online", cx).unwrap();
    assert_eq!(
        shell_requests(),
        [Shell::Browse(
            "https://duckduckgo.com/?q=missing.txt".into()
        )]
    );
    cx.update(|_, cx| {
        let engine = &mut cx.global_mut::<AppState>().settings.search_engine;
        engine.engine = birchpad_config::SearchEngine::Custom;
        engine.custom_url = "ftp://example.com/".into();
    });
    assert!(command(&workspace, "edit.search-online", cx).is_err());
    assert!(shell_requests().is_empty());
}

#[gpui_kit::test]
fn inserting_the_date_and_time(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().settings.date.custom_format = "'year' yyyy".into();
    });
    let year = chrono::Datelike::year(&chrono::Local::now());
    run(
        &workspace,
        Invocation::with_args("edit.insert-date-time", json!({ "format": "custom" })),
        cx,
    )
    .unwrap();
    assert_eq!(active_text(&workspace, cx), format!("year {year}"));
    // Short: "HH:mm yyyy-MM-dd", at every caret.
    cx.simulate_keystrokes(&secondary("a"));
    run(
        &workspace,
        Invocation::with_args("edit.insert-date-time", json!({ "format": "short" })),
        cx,
    )
    .unwrap();
    let text = active_text(&workspace, cx);
    assert_eq!(text.len(), "HH:mm yyyy-MM-dd".len(), "{text}");
    assert_eq!((&text[2..3], &text[5..6]), (":", " "), "{text}");
    assert_eq!(&text[6..10], year.to_string(), "{text}");
    // An unknown format, or none: refused.
    for args in [json!({ "format": "medium" }), json!({})] {
        assert!(
            run(
                &workspace,
                Invocation::with_args("edit.insert-date-time", args),
                cx
            )
            .is_err()
        );
    }
}

#[gpui_kit::test]
fn tab_and_shift_tab_indent_lines(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("a\nb");
    // Within a line, Tab types a tab.
    cx.simulate_keystrokes("home tab");
    assert_eq!(active_text(&workspace, cx), "a\n\tb");
    // Over several lines, it indents them; Shift+Tab takes it back.
    select_all(cx);
    cx.simulate_keystrokes("tab");
    assert_eq!(active_text(&workspace, cx), "\ta\n\t\tb");
    cx.simulate_keystrokes("shift-tab shift-tab");
    assert_eq!(active_text(&workspace, cx), "a\nb");
    command(&workspace, "edit.indent", cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "\ta\n\tb");
    // With spaces instead of tabs.
    cx.update(|_, cx| {
        let editor = &mut cx.global_mut::<AppState>().settings.editor;
        editor.insert_spaces = true;
        editor.tab_width = 2;
    });
    command(&workspace, "edit.outdent", cx).unwrap();
    command(&workspace, "edit.indent", cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "  a\n  b");
    // Read-only: nothing moves.
    command(&workspace, "edit.toggle-read-only", cx).unwrap();
    command(&workspace, "edit.outdent", cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "  a\n  b");
}

#[gpui_kit::test]
fn on_selection_takes_absolute_and_quoted_paths(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target file.txt");
    fs::write(&target, "target").unwrap();
    let (workspace, cx) = open_workspace(cx);
    let insert = |text: String, cx: &mut VisualTestContext| {
        select_all(cx);
        run(
            &workspace,
            Invocation::with_args("edit.insert-text", json!({ "text": text })),
            cx,
        )
        .unwrap();
        select_all(cx);
    };
    // An absolute path, in single quotes, in an unsaved document.
    insert(format!(" '{}' ", target.display()), cx);
    command(&workspace, "edit.open-selected-file", cx).unwrap();
    assert_eq!(tab_names(&workspace, cx).last().unwrap(), "target file.txt");
    // A folder is no file to open, but shown by Open Containing Folder.
    workspace.update_in(cx, |workspace, window, cx| workspace.new_file(window, cx));
    insert(format!("\"{}\"", dir.path().display()), cx);
    let error = command(&workspace, "edit.open-selected-file", cx).unwrap_err();
    assert!(
        error
            .to_string()
            .contains(&dir.path().display().to_string())
    );
    shell_requests();
    command(&workspace, "edit.open-selected-folder", cx).unwrap();
    assert!(matches!(&shell_requests()[..], [Shell::Reveal(path)] if path == dir.path()));
    // Only blanks selected: nothing to take.
    insert("  \t ".into(), cx);
    let error = command(&workspace, "edit.open-selected-file", cx).unwrap_err();
    assert!(error.to_string().contains("select"), "{error}");
}
