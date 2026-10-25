//! The menu items left from Notepad++ 8 (roadmap 7.4a).

use std::path::Path;

use anyhow::Result;
use birchpad::app_state::AppState;
use birchpad::workspace::Workspace;
use birchpad_commands::Invocation;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use crate::workspace::{active_text, document_start, open_workspace};

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

fn select_all(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) {
    command(workspace, "edit.select-all", cx);
}

fn requested_read_only(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<bool> {
    workspace.read_with(cx, |workspace, cx| {
        workspace
            .all_views(cx)
            .iter()
            .map(|view| view.read(cx).buffer.read(cx).requested_read_only())
            .collect()
    })
}

fn open(workspace: &Entity<Workspace>, path: &Path, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(path, window, cx)
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn read_only_for_every_document_and_back(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    command(&workspace, "file.new", cx);
    command(&workspace, "edit.set-read-only-all", cx);
    assert_eq!(requested_read_only(&workspace, cx), [true, true]);
    // Typing changes nothing now.
    cx.simulate_input("x");
    assert_eq!(active_text(&workspace, cx), "");
    command(&workspace, "edit.clear-read-only-all", cx);
    assert_eq!(requested_read_only(&workspace, cx), [false, false]);
    command(&workspace, "edit.toggle-read-only", cx);
    assert_eq!(requested_read_only(&workspace, cx), [false, true]);
}

#[gpui_kit::test]
fn the_file_s_read_only_attribute_toggles(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "text").unwrap();
    let (workspace, cx) = open_workspace(cx);
    open(&workspace, &path, cx);
    command(&workspace, "edit.toggle-read-only-attribute", cx);
    assert!(std::fs::metadata(&path).unwrap().permissions().readonly());
    let read_only = |cx: &mut VisualTestContext| {
        workspace.read_with(cx, |workspace, cx| {
            workspace
                .active_view(cx)
                .unwrap()
                .read(cx)
                .buffer
                .read(cx)
                .read_only()
        })
    };
    assert_eq!(read_only(cx), Some(birchpad::buffer::ReadOnly::File));
    command(&workspace, "edit.toggle-read-only-attribute", cx);
    assert!(!std::fs::metadata(&path).unwrap().permissions().readonly());
    assert_eq!(read_only(cx), None);
}

#[gpui_kit::test]
fn an_untitled_document_has_no_attribute_to_toggle(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    assert!(
        run(
            &workspace,
            Invocation::new("edit.toggle-read-only-attribute"),
            cx
        )
        .is_err()
    );
}

#[gpui_kit::test]
fn redact_selection_hides_the_selected_text(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("secret\nend");
    select_all(&workspace, cx);
    command(&workspace, "edit.redact-selection", cx);
    assert_eq!(active_text(&workspace, cx), "██████\n███");
    // Undo brings the text back.
    command(&workspace, "edit.undo", cx);
    assert_eq!(active_text(&workspace, cx), "secret\nend");
}

#[gpui_kit::test]
fn lines_sort_in_locale_order_from_the_menu(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("b\nÁ\na\nB");
    let sort = |descending: bool| {
        Invocation::with_args(
            "edit.sort-lines",
            json!({ "by": "locale", "descending": descending }),
        )
    };
    // Typed Enter breaks lines as the platform does.
    let text = |cx: &mut VisualTestContext| active_text(&workspace, cx).replace("\r\n", "\n");
    run(&workspace, sort(false), cx).unwrap();
    assert_eq!(text(cx), "a\nÁ\nb\nB");
    run(&workspace, sort(true), cx).unwrap();
    assert_eq!(text(cx), "B\nb\nÁ\na");
    let menu_has = birchpad_commands::main_menu()
        .iter()
        .flat_map(|menu| menu.invocations().into_iter().cloned().collect::<Vec<_>>())
        .any(|invocation| invocation == sort(false));
    assert!(menu_has, "Line Operations lists it");
}

#[gpui_kit::test]
fn column_mode_and_change_search_engine_open_their_dialogs(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    command(&workspace, "edit.column-mode", cx);
    assert!(cx.update(|window, cx| gpui_kit::component::WindowExt::has_active_dialog(window, cx)));
    cx.update(|window, cx| gpui_kit::component::WindowExt::close_dialog(window, cx));
    command(&workspace, "edit.change-search-engine", cx);
    let preferences = birchpad::preferences::OPENED
        .with(|opened| opened.borrow().clone().and_then(|weak| weak.upgrade()))
        .expect("Preferences is open");
    let shown = preferences.read_with(cx, |preferences, _| preferences.section);
    assert_eq!(birchpad::preferences::sections()[shown], "Search Engine");
}

#[gpui_kit::test]
fn show_symbol_switches_for_non_printing_and_control_characters(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    // Typing keeps control characters out (Prevent typing control characters): inserted.
    run(
        &workspace,
        Invocation::with_args(
            "edit.insert-text",
            json!({ "text": "a\u{A0}b\u{200B}c\u{1}" }),
        ),
        cx,
    )
    .unwrap();
    let shown = |cx: &mut VisualTestContext| {
        workspace.read_with(cx, |workspace, cx| {
            let view = workspace.active_view(cx).unwrap();
            let layout = view.read(cx).layout.as_ref().expect("drawn");
            (
                layout.symbols.odd_spaces.len(),
                layout.symbols.zero_widths.len(),
                layout.rows[0].display.text.clone(),
            )
        })
    };
    let (spaces, marks, text) = shown(cx);
    assert_eq!((spaces, marks), (0, 0), "off by default");
    assert!(
        text.ends_with('\u{2401}'),
        "control pictures on by default: {text:?}"
    );
    command(&workspace, "view.show-non-printing", cx);
    command(&workspace, "view.show-control-chars", cx);
    let (spaces, marks, text) = shown(cx);
    assert_eq!((spaces, marks), (1, 1));
    assert!(text.ends_with(' '), "{text:?}");
    let state = cx.update(|_, cx| AppState::global(cx).state.clone());
    assert_eq!(
        (state.show_non_printing, state.show_control_chars),
        (Some(true), Some(false))
    );
    // The text is unchanged.
    assert_eq!(active_text(&workspace, cx), "a\u{A0}b\u{200B}c\u{1}");
}

#[gpui_kit::test]
fn zoom_on_its_own_when_views_are_not_synchronized(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    command(&workspace, "file.new", cx);
    let zooms = |cx: &mut VisualTestContext| {
        workspace.read_with(cx, |workspace, cx| {
            workspace
                .all_views(cx)
                .iter()
                .map(|view| view.read(cx).zoom)
                .collect::<Vec<_>>()
        })
    };
    command(&workspace, "view.zoom-in", cx);
    assert_eq!(cx.update(|_, cx| AppState::global(cx).state.zoom), 1);
    assert_eq!(zooms(cx), [None, None]);
    command(&workspace, "view.zoom-sync", cx);
    command(&workspace, "view.zoom-in", cx);
    command(&workspace, "view.zoom-in", cx);
    assert_eq!(cx.update(|_, cx| AppState::global(cx).state.zoom), 1);
    assert_eq!(zooms(cx), [None, Some(3)]);
    command(&workspace, "view.zoom-reset", cx);
    assert_eq!(zooms(cx), [None, Some(0)]);
    // Synchronized again: every view takes the shared zoom.
    command(&workspace, "view.zoom-sync", cx);
    assert_eq!(zooms(cx), [None, None]);
    assert!(!cx.update(|_, cx| AppState::global(cx).state.zoom_per_view));
}

#[test]
fn hashes_of_a_text_or_of_each_line() {
    use birchpad::tools::text_hashes;
    use birchpad_core::hash::HashKind;
    let md5 = |text: &str, per_line| text_hashes(HashKind::Md5, text, per_line);
    assert_eq!(md5("abc", false), "900150983cd24fb0d6963f7d28e17f72");
    assert_eq!(md5("", false), "d41d8cd98f00b204e9800998ecf8427e");
    assert_eq!(
        md5("abc\n\nabc", true),
        "900150983cd24fb0d6963f7d28e17f72\nd41d8cd98f00b204e9800998ecf8427e\n900150983cd24fb0d6963f7d28e17f72"
    );
    // No lines at all: nothing.
    assert_eq!(md5("", true), "");
}

#[gpui_kit::test]
fn generate_opens_a_dialog_that_hashes_what_is_typed(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    run(
        &workspace,
        Invocation::with_args("tools.hash-text", json!({ "hash": "md5" })),
        cx,
    )
    .unwrap();
    let dialog = birchpad::tools::HASH_TEXT
        .with(|opened| opened.borrow().clone().and_then(|weak| weak.upgrade()))
        .expect("the dialog is open");
    let input = dialog.read_with(cx, |dialog, _| dialog.input.clone());
    input.update_in(cx, |input, window, cx| input.set_value("abc", window, cx));
    let hashes = dialog.read_with(cx, |dialog, cx| dialog.hashes(cx));
    assert_eq!(hashes, "900150983cd24fb0d6963f7d28e17f72");
    dialog.update(cx, |dialog, cx| dialog.copy(cx));
    let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(copied.as_deref(), Some("900150983cd24fb0d6963f7d28e17f72"));
}

fn completion_labels(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.read(cx)
            .completion
            .as_ref()
            .map(|popup| popup.items.iter().map(|item| item.label.clone()).collect())
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
fn word_completion_offers_the_document_s_words(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("alpha alpine\nal");
    command(&workspace, "completion.words", cx);
    assert_eq!(completion_labels(&workspace, cx), ["alpha", "alpine"]);
    // Nothing typed: nothing offered.
    cx.simulate_input(" ");
    command(&workspace, "completion.words", cx);
    assert!(completion_labels(&workspace, cx).is_empty());
}

#[gpui_kit::test]
fn path_completion_lists_the_folder_s_names(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src").join("main.rs"), "").unwrap();
    std::fs::write(dir.path().join("src").join("lib.rs"), "").unwrap();
    let path = dir.path().join("notes.txt");
    std::fs::write(&path, "").unwrap();
    let (workspace, cx) = open_workspace(cx);
    open(&workspace, &path, cx);
    cx.simulate_input("see src/m");
    command(&workspace, "completion.path", cx);
    assert_eq!(completion_labels(&workspace, cx), ["main.rs"]);
    command(&workspace, "completion.accept", cx);
    assert_eq!(active_text(&workspace, cx), "see src/main.rs");
    // An absolute path; a folder that is not there offers nothing.
    cx.simulate_keystrokes(document_start());
    cx.simulate_input(&format!("{} ", dir.path().join("sr").display()));
    cx.simulate_keystrokes("left");
    command(&workspace, "completion.path", cx);
    let folder = format!("src{}", std::path::MAIN_SEPARATOR);
    assert_eq!(completion_labels(&workspace, cx), [folder]);
    cx.simulate_input("x/missing/");
    command(&workspace, "completion.path", cx);
    assert!(completion_labels(&workspace, cx).is_empty());
}

#[gpui_kit::test]
fn path_completion_needs_a_folder_for_relative_paths(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("src/m");
    command(&workspace, "completion.path", cx);
    assert!(completion_labels(&workspace, cx).is_empty());
}
