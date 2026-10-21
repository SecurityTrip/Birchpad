use gpui_kit::{MouseButton, TestAppContext, VisualTestContext};

use crate::workspace::{click_on, open_workspace};
use birchpad::path_dialog::*;
use futures::channel::oneshot;
use gpui_kit::component::WindowExt as _;

fn open(initial: &str, cx: &mut VisualTestContext) -> oneshot::Receiver<Option<String>> {
    let (sender, receiver) = oneshot::channel();
    cx.update(|window, cx| {
        open_text_dialog("Save As", initial.to_owned(), sender, window, cx);
    });
    cx.run_until_parked();
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    receiver
}

#[gpui_kit::test]
fn enter_answers_with_the_path(cx: &mut TestAppContext) {
    let (_workspace, cx) = open_workspace(cx);
    let mut receiver = open("C:/notes/a.txt", cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(
        receiver.try_recv().unwrap(),
        Some(Some("C:/notes/a.txt".to_owned()))
    );
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
}

#[gpui_kit::test]
fn escape_cancels(cx: &mut TestAppContext) {
    let (_workspace, cx) = open_workspace(cx);
    let mut receiver = open("C:/notes/a.txt", cx);
    cx.simulate_keystrokes("escape");
    assert_eq!(receiver.try_recv().unwrap(), Some(None));
}

#[gpui_kit::test]
fn typing_replaces_the_suggestion(cx: &mut TestAppContext) {
    let (_workspace, cx) = open_workspace(cx);
    let mut receiver = open("Project Name", cx);
    cx.simulate_input("Editor");
    cx.simulate_keystrokes("enter");
    assert_eq!(
        receiver.try_recv().unwrap(),
        Some(Some("Editor".to_owned()))
    );

    // The field keeps the focus: arrows and Backspace edit the suggestion itself.
    let mut receiver = open("notes.txt", cx);
    cx.simulate_keystrokes("end backspace backspace backspace backspace");
    cx.simulate_input(".md");
    cx.simulate_keystrokes("enter");
    assert_eq!(
        receiver.try_recv().unwrap(),
        Some(Some("notes.md".to_owned()))
    );

    // An empty suggestion takes the typing as it is.
    let mut receiver = open("", cx);
    cx.simulate_input("x");
    cx.simulate_keystrokes("enter");
    assert_eq!(receiver.try_recv().unwrap(), Some(Some("x".to_owned())));
}

#[gpui_kit::test]
fn an_empty_path_is_not_an_answer(cx: &mut TestAppContext) {
    let (_workspace, cx) = open_workspace(cx);
    let mut receiver = open("   ", cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(receiver.try_recv().unwrap(), None, "still asking");
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    cx.simulate_keystrokes("escape");
    assert_eq!(receiver.try_recv().unwrap(), Some(None));
}

#[gpui_kit::test]
fn its_buttons_answer_or_cancel_on_a_click(cx: &mut TestAppContext) {
    let (_workspace, cx) = open_workspace(cx);
    let mut receiver = open("C:/notes/a.txt", cx);
    click_on("dialog-action", MouseButton::Left, cx);
    assert_eq!(
        receiver.try_recv().unwrap(),
        Some(Some("C:/notes/a.txt".to_owned()))
    );
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));

    // An empty path is no answer for the mouse either; Cancel then answers nothing.
    let mut receiver = open("", cx);
    click_on("dialog-action", MouseButton::Left, cx);
    assert_eq!(receiver.try_recv().unwrap(), None, "still asking");
    click_on("dialog-close", MouseButton::Left, cx);
    assert_eq!(receiver.try_recv().unwrap(), Some(None));
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));

    // Only the left button presses them.
    let mut receiver = open("C:/b.txt", cx);
    click_on("dialog-action", MouseButton::Right, cx);
    click_on("dialog-close", MouseButton::Middle, cx);
    assert_eq!(receiver.try_recv().unwrap(), None);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
}

#[gpui_kit::test]
fn spaces_around_the_path_are_dropped(cx: &mut TestAppContext) {
    let (_workspace, cx) = open_workspace(cx);
    let mut receiver = open("  D:/b.txt  ", cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(
        receiver.try_recv().unwrap(),
        Some(Some("D:/b.txt".to_owned()))
    );
}

/// Without the system's dialogs, the in-app one asks: for files to open, a place to save,
/// a folder.
#[gpui_kit::test]
fn without_the_system_dialogs_the_path_is_typed(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    NO_SYSTEM_DIALOG.set(true);
    let dir = tempfile::tempdir().unwrap();
    let notes = dir.path().join("notes.txt");
    std::fs::write(&notes, "kept").unwrap();
    let run = |command: &str, cx: &mut VisualTestContext| {
        let invocation = birchpad_commands::Invocation::new(command);
        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.dispatch(&invocation, window, cx)
            })
            .unwrap();
        cx.run_until_parked();
    };

    run("file.open", cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    cx.simulate_input(&notes.display().to_string());
    cx.simulate_keystrokes("enter");
    assert_eq!(crate::workspace::active_text(&workspace, cx), "kept");

    run("file.new", cx);
    cx.simulate_input("new text");
    let saved = dir.path().join("saved.txt");
    run("file.save-as", cx);
    cx.simulate_input(&saved.display().to_string());
    cx.simulate_keystrokes("enter");
    assert_eq!(std::fs::read_to_string(&saved).unwrap(), "new text");

    // Cancelled: nothing is opened.
    let tabs = crate::workspace::tab_names(&workspace, cx).len();
    run("file.open", cx);
    cx.simulate_keystrokes("escape");
    assert_eq!(crate::workspace::tab_names(&workspace, cx).len(), tabs);

    // The folder of Find in Files.
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.browse_folder(window, cx)
    });
    cx.run_until_parked();
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    cx.simulate_input(&dir.path().display().to_string());
    cx.simulate_keystrokes("enter");
    let typed = workspace.read_with(cx, |workspace, cx| {
        workspace.find_bar.read(cx).directory(cx)
    });
    assert_eq!(typed, dir.path().display().to_string());
    NO_SYSTEM_DIALOG.set(false);
}
