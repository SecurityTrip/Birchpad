use std::fs;

use birchpad_commands::Invocation;
use gpui_kit::{TestAppContext, VisualTestContext};

use super::*;
use crate::workspace::tests::{active_text, open_workspace, secondary, tab_names};

fn open(workspace: &Entity<Workspace>, path: &Path, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(path, window, cx)
    });
    cx.run_until_parked();
}

fn is_modified(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.read(cx).buffer.read(cx).is_modified()
    })
}

fn recent(cx: &mut VisualTestContext) -> Vec<PathBuf> {
    cx.update(|_, cx| AppState::global(cx).state.recent_files.clone())
}

#[gpui_kit::test]
fn save_writes_the_file_in_its_encoding(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cp1251.txt");
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../io/tests/fixtures/windows-1251-crlf.txt");
    let original = fs::read(fixture).unwrap();
    fs::write(&path, &original).unwrap();
    let (workspace, cx) = open_workspace(cx);
    open(&workspace, &path, cx);
    assert!(active_text(&workspace, cx).starts_with("Съешь"));

    cx.simulate_input("!");
    assert!(is_modified(&workspace, cx));
    cx.simulate_keystrokes(&secondary("s"));
    cx.run_until_parked();
    assert!(!is_modified(&workspace, cx));
    let mut expected = b"!".to_vec();
    expected.extend_from_slice(&original);
    assert_eq!(fs::read(&path).unwrap(), expected);

    // Characters Windows-1251 lacks block the save and leave the file alone.
    cx.simulate_input("日");
    cx.simulate_keystrokes(&secondary("s"));
    cx.run_until_parked();
    assert!(is_modified(&workspace, cx));
    assert_eq!(fs::read(&path).unwrap(), expected);
}

#[gpui_kit::test]
fn saving_an_untitled_document_asks_for_a_path(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("notes.txt");
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("hello");
    cx.simulate_keystrokes(&secondary("s"));
    cx.run_until_parked();
    let chosen = target.clone();
    cx.simulate_new_path_selection(move |_| Some(chosen));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&target).unwrap(), "hello");
    assert_eq!(tab_names(&workspace, cx), ["notes.txt"]);
    assert!(!is_modified(&workspace, cx));
}

#[gpui_kit::test]
fn closing_a_modified_document_asks_first(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_keystrokes(&secondary("n"));
    cx.simulate_input("unsaved");
    assert_eq!(tab_names(&workspace, cx), ["new 1", "new 2"]);

    cx.simulate_keystrokes(&secondary("w"));
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx), ["new 1", "new 2"]);

    cx.simulate_keystrokes(&secondary("w"));
    cx.run_until_parked();
    cx.simulate_prompt_answer("Don't Save");
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx), ["new 1"]);

    // An unmodified document closes without a question.
    cx.simulate_keystrokes(&secondary("w"));
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
}

#[gpui_kit::test]
fn close_all_saves_when_asked_and_stops_on_cancel(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.txt");
    let second = dir.path().join("second.txt");
    fs::write(&first, "1").unwrap();
    fs::write(&second, "2").unwrap();
    let (workspace, cx) = open_workspace(cx);
    open(&workspace, &first, cx);
    cx.simulate_input("a");
    open(&workspace, &second, cx);
    cx.simulate_input("b");

    workspace.update_in(cx, |workspace, window, cx| {
        workspace
            .dispatch(&Invocation::new("file.close-all"), window, cx)
            .unwrap();
    });
    cx.run_until_parked();
    cx.simulate_prompt_answer("Save");
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&first).unwrap(), "a1");
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx), ["second.txt"]);
    assert_eq!(fs::read_to_string(&second).unwrap(), "2");
    assert_eq!(
        recent(cx),
        std::slice::from_ref(&first),
        "closed files become recent"
    );

    // Reopening a recent file removes it from the list.
    open(&workspace, &first, cx);
    assert!(recent(cx).is_empty());
}

#[gpui_kit::test]
fn open_dialog_opens_every_chosen_file(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let paths = [dir.path().join("a.txt"), dir.path().join("b.txt")];
    for path in &paths {
        fs::write(path, "x").unwrap();
    }
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_keystrokes(&secondary("o"));
    cx.run_until_parked();
    let chosen = paths.to_vec();
    cx.simulate_path_prompt_response(move |_| Some(chosen));
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx), ["a.txt", "b.txt"]);
}

#[gpui_kit::test]
fn dropped_files_open_in_tabs(cx: &mut TestAppContext) {
    use gpui_kit::{ExternalPaths, FileDropEvent, point, px};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dropped.txt");
    fs::write(&path, "dropped").unwrap();
    let (workspace, cx) = open_workspace(cx);
    let position = point(px(300.), px(300.));
    cx.simulate_event(FileDropEvent::Entered {
        position,
        paths: ExternalPaths(vec![path.clone()].into()),
    });
    cx.simulate_event(FileDropEvent::Submit { position });
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx), ["dropped.txt"]);
    assert_eq!(active_text(&workspace, cx), "dropped");
}

#[cfg(unix)]
#[gpui_kit::test]
fn read_only_files_cannot_be_edited_or_saved(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ro.txt");
    fs::write(&path, "fixed").unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions).unwrap();

    let (workspace, cx) = open_workspace(cx);
    open(&workspace, &path, cx);
    cx.simulate_input("changed");
    assert_eq!(active_text(&workspace, cx), "fixed");
    cx.simulate_keystrokes(&secondary("s"));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&path).unwrap(), "fixed");
}
