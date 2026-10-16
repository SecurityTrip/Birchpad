use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use birchpad_commands::Invocation;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use super::*;
use crate::workspace::tests::{active_text, open_workspace, secondary, tab_names, take_info};

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

fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, text).unwrap();
    path
}

fn active(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<EditorView> {
    workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap())
}

fn active_path(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<PathBuf> {
    active(workspace, cx).read_with(cx, |view, cx| {
        view.buffer.read(cx).path().map(Path::to_owned)
    })
}

fn modified(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    active(workspace, cx).read_with(cx, |view, cx| view.buffer.read(cx).is_modified())
}

fn shell_requests() -> Vec<Shell> {
    SHELL.with(|shell| std::mem::take(&mut *shell.borrow_mut()))
}

fn clipboard(cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
}

fn recent(cx: &mut VisualTestContext) -> Vec<PathBuf> {
    cx.update(|_, cx| AppState::global(cx).state.recent_files.clone())
}

fn folder(target: &str) -> Invocation {
    Invocation::with_args("file.containing-folder", json!({ "in": target }))
}

#[gpui_kit::test]
fn the_containing_folder_and_the_default_viewer(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let file = write(dir.path(), "a.txt", "a");
    let (workspace, cx) = open_workspace(cx);

    // An untitled document has no folder and nothing to view.
    for invocation in [
        folder("file-manager"),
        folder("terminal"),
        folder("workspace"),
        Invocation::new("file.open-default-viewer"),
        Invocation::new("edit.copy-folder-path"),
    ] {
        let error = run(&workspace, invocation.clone(), cx).unwrap_err();
        assert!(
            error.to_string().contains("new 1"),
            "{invocation:?}: {error}"
        );
    }
    assert!(shell_requests().is_empty());

    open(&workspace, &file, cx);
    let file = active_path(&workspace, cx).unwrap();
    let parent = file.parent().unwrap().to_owned();
    run(&workspace, folder("file-manager"), cx).unwrap();
    run(&workspace, folder("terminal"), cx).unwrap();
    command(&workspace, "file.open-default-viewer", cx).unwrap();
    assert_eq!(
        shell_requests(),
        [
            Shell::Reveal(file.clone()),
            Shell::Terminal(parent.clone()),
            Shell::Open(file.clone())
        ]
    );
    run(&workspace, folder("workspace"), cx).unwrap();
    let panel = workspace.read_with(cx, |workspace, _| {
        crate::panels::is_checked(
            &workspace.docks,
            &Invocation::new("panel.folder-as-workspace"),
        )
    });
    assert_eq!(panel, Some(true));

    // A place it does not know, or none.
    assert!(run(&workspace, folder("browser"), cx).is_err());
    assert!(command(&workspace, "file.containing-folder", cx).is_err());
    assert!(shell_requests().is_empty());
}

#[gpui_kit::test]
fn copying_the_path_name_and_folder(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let file = write(dir.path(), "notes.md", "x");
    let (workspace, cx) = open_workspace(cx);
    // Untitled: its name for the path and the file name.
    command(&workspace, "edit.copy-full-path", cx).unwrap();
    assert_eq!(clipboard(cx).as_deref(), Some("new 1"));
    command(&workspace, "edit.copy-file-name", cx).unwrap();
    assert_eq!(clipboard(cx).as_deref(), Some("new 1"));

    open(&workspace, &file, cx);
    let file = active_path(&workspace, cx).unwrap();
    command(&workspace, "edit.copy-full-path", cx).unwrap();
    assert_eq!(clipboard(cx), Some(file.display().to_string()));
    command(&workspace, "edit.copy-file-name", cx).unwrap();
    assert_eq!(clipboard(cx).as_deref(), Some("notes.md"));
    command(&workspace, "edit.copy-folder-path", cx).unwrap();
    assert_eq!(
        clipboard(cx),
        Some(file.parent().unwrap().display().to_string())
    );
}

#[gpui_kit::test]
fn reloading_from_disk(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let file = write(dir.path(), "a.txt", "one");
    let (workspace, cx) = open_workspace(cx);
    assert!(command(&workspace, "file.reload", cx).is_err(), "untitled");

    open(&workspace, &file, cx);
    // Unchanged: read again without a question.
    fs::write(&file, "two").unwrap();
    command(&workspace, "file.reload", cx).unwrap();
    assert!(!cx.has_pending_prompt());
    assert_eq!(active_text(&workspace, cx), "two");

    // With changes: asked first; Cancel keeps them.
    cx.simulate_input("x");
    command(&workspace, "file.reload", cx).unwrap();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert_eq!(active_text(&workspace, cx), "xtwo");
    command(&workspace, "file.reload", cx).unwrap();
    cx.simulate_prompt_answer("Reload");
    cx.run_until_parked();
    assert_eq!(active_text(&workspace, cx), "two");
    assert!(!modified(&workspace, cx));

    // A file that is gone: an error, the text stays.
    fs::remove_file(&file).unwrap();
    let error = command(&workspace, "file.reload", cx).unwrap_err();
    assert!(error.to_string().contains("no longer exists"), "{error}");
    assert_eq!(active_text(&workspace, cx), "two");
}

/// Ctrl+R (Cmd+R on macOS) reloads the file, asking first when it has changes; in an untitled
/// document it does nothing.
#[gpui_kit::test]
fn ctrl_r_reloads_from_disk(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let file = write(dir.path(), "a.txt", "one");
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_keystrokes(&secondary("r"));
    assert!(!cx.has_pending_prompt());
    assert_eq!(active_text(&workspace, cx), "", "untitled: nothing to read");

    open(&workspace, &file, cx);
    fs::write(&file, "two").unwrap();
    cx.simulate_keystrokes(&secondary("r"));
    cx.run_until_parked();
    assert_eq!(active_text(&workspace, cx), "two");

    cx.simulate_input("x");
    cx.simulate_keystrokes(&secondary("r"));
    cx.run_until_parked();
    assert!(cx.has_pending_prompt(), "changes: asked first");
    cx.simulate_prompt_answer("Reload");
    cx.run_until_parked();
    assert_eq!(active_text(&workspace, cx), "two");
    // One binding: Ctrl+Shift+R (macro recording) and Ctrl+Alt+R (case) are other commands.
    let keymap = cx.update(|_, cx| cx.global::<crate::commands::ActiveKeymap>().0.clone());
    let reload = birchpad_commands::Invocation::new("file.reload");
    let keys: Vec<String> = keymap
        .bindings_for(&reload)
        .map(|binding| binding.keys_string())
        .collect();
    assert_eq!(keys, [secondary("r")]);
}

#[gpui_kit::test]
fn saving_a_copy_leaves_the_document_as_it_is(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    // UTF-16 with a BOM: the copy is in the document's encoding.
    let mut bytes = vec![0xff, 0xfe];
    bytes.extend("hi".encode_utf16().flat_map(u16::to_le_bytes));
    let file = dir.path().join("a.txt");
    fs::write(&file, &bytes).unwrap();
    let other = write(dir.path(), "other.txt", "other");
    let (workspace, cx) = open_workspace(cx);
    open(&workspace, &file, cx);
    let file = active_path(&workspace, cx).unwrap();
    cx.simulate_input("!");

    let copy = dir.path().join("copy.txt");
    command(&workspace, "file.save-copy-as", cx).unwrap();
    let chosen = copy.clone();
    cx.simulate_new_path_selection(move |_| Some(chosen));
    cx.run_until_parked();
    let mut expected = vec![0xff, 0xfe];
    expected.extend("!hi".encode_utf16().flat_map(u16::to_le_bytes));
    assert_eq!(fs::read(&copy).unwrap(), expected);
    assert_eq!(
        fs::read(&file).unwrap(),
        bytes,
        "the file itself is unchanged"
    );
    assert_eq!(active_path(&workspace, cx).as_deref(), Some(file.as_path()));
    assert!(modified(&workspace, cx));

    // Cancelled: nothing written.
    fs::remove_file(&copy).unwrap();
    command(&workspace, "file.save-copy-as", cx).unwrap();
    cx.simulate_new_path_selection(|_| None);
    cx.run_until_parked();
    assert!(!copy.exists());

    // A file open in another tab is refused; the document's own file is a plain save.
    open(&workspace, &other, cx);
    let other = active_path(&workspace, cx).unwrap();
    crate::workspace::tests::panes(&workspace, cx);
    run(
        &workspace,
        Invocation::with_args("view.go-to-tab", json!({ "index": 1 })),
        cx,
    )
    .unwrap();
    let view = active(&workspace, cx);
    let refused = workspace.update_in(cx, |workspace, window, cx| {
        workspace.write_copy(&view, &other, window, cx)
    });
    assert!(refused.is_err());
    assert_eq!(fs::read_to_string(&other).unwrap(), "other");
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.write_copy(&view, &file, window, cx)
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(fs::read(&file).unwrap(), expected);
    assert!(!modified(&workspace, cx));
}

fn rename(workspace: &Entity<Workspace>, name: &str, cx: &mut VisualTestContext) -> Result<()> {
    let view = active(workspace, cx);
    let result = workspace.update(cx, |workspace, cx| workspace.rename(&view, name, cx));
    cx.run_until_parked();
    result
}

#[gpui_kit::test]
fn renaming_a_file(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let file = write(dir.path(), "a.txt", "fn main() {}");
    write(dir.path(), "taken.txt", "");
    let (workspace, cx) = open_workspace(cx);
    open(&workspace, &file, cx);
    let file = active_path(&workspace, cx).unwrap();

    // The file follows, and its new extension gives the language.
    rename(&workspace, "main.rs", cx).unwrap();
    let renamed = file.with_file_name("main.rs");
    assert!(!file.exists() && renamed.exists());
    assert_eq!(tab_names(&workspace, cx), ["main.rs"]);
    assert_eq!(active_path(&workspace, cx), Some(renamed.clone()));
    let language = active(&workspace, cx)
        .read_with(cx, |view, cx| view.buffer.read(cx).language().map(|l| l.id));
    assert_eq!(language, Some("rust"));

    // The same name, or surrounding spaces: nothing to do.
    rename(&workspace, " main.rs ", cx).unwrap();
    assert!(renamed.exists());

    // Refused: an existing file, an empty name, folders.
    for name in ["taken.txt", "", "  ", "sub/x.rs", "..\\x.rs", ".."] {
        assert!(rename(&workspace, name, cx).is_err(), "{name:?}");
    }
    assert!(renamed.exists());
    assert_eq!(tab_names(&workspace, cx), ["main.rs"]);
    // Saving goes to the new file.
    cx.simulate_input("// ");
    cx.simulate_keystrokes(&secondary("s"));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&renamed).unwrap(), "// fn main() {}");
}

#[gpui_kit::test]
fn renaming_an_untitled_document(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    rename(&workspace, "Ideas", cx).unwrap();
    assert_eq!(tab_names(&workspace, cx), ["Ideas"]);
    // Its number is still taken.
    cx.simulate_keystrokes(&secondary("n"));
    assert_eq!(tab_names(&workspace, cx), ["Ideas", "new 2"]);
    assert!(rename(&workspace, "a/b", cx).is_err());
    assert_eq!(tab_names(&workspace, cx), ["Ideas", "new 2"]);

    // The dialog opens with the current name.
    command(&workspace, "file.rename", cx).unwrap();
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
}

#[test]
fn names_of_files() {
    for good in ["a", "a.txt", ".gitignore", "имя файла.md", "a..b"] {
        assert!(check_name(good).is_ok(), "{good}");
    }
    for bad in ["", " ", "a/b", "a\\b", ".", "..", "/"] {
        assert!(check_name(bad).is_err(), "{bad:?}");
    }
}

#[gpui_kit::test]
fn moving_to_the_recycle_bin(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let file = write(dir.path(), "a.txt", "a");
    let (workspace, cx) = open_workspace(cx);
    assert!(
        command(&workspace, "file.move-to-recycle-bin", cx).is_err(),
        "untitled"
    );
    open(&workspace, &file, cx);
    cx.simulate_input("unsaved ");

    // Cancel: the file and its tab stay.
    command(&workspace, "file.move-to-recycle-bin", cx).unwrap();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert!(file.exists());
    assert_eq!(tab_names(&workspace, cx), ["a.txt"]);

    // Move: the file goes, its tab closes without asking, and it is not a recent file.
    command(&workspace, "file.move-to-recycle-bin", cx).unwrap();
    cx.simulate_prompt_answer("Move");
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    assert!(!file.exists());
    assert_eq!(tab_names(&workspace, cx), ["new 1"]);
    assert!(recent(cx).is_empty());

    // A file already gone: the error is shown and the tab stays.
    let gone = write(dir.path(), "b.txt", "b");
    open(&workspace, &gone, cx);
    fs::remove_file(&gone).unwrap();
    command(&workspace, "file.move-to-recycle-bin", cx).unwrap();
    cx.simulate_prompt_answer("Move");
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx), ["b.txt"]);
}

#[gpui_kit::test]
fn reopening_recent_files(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let [a, b, c] = ["a.txt", "b.txt", "c.txt"].map(|name| write(dir.path(), name, name));
    let (workspace, cx) = open_workspace(cx);
    for path in [&a, &b, &c] {
        open(&workspace, path, cx);
    }
    for _ in 0..3 {
        command(&workspace, "file.close", cx).unwrap();
    }
    assert_eq!(tab_names(&workspace, cx), ["new 1"]);
    let names = |paths: Vec<PathBuf>| -> Vec<String> {
        paths
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    };
    assert_eq!(names(recent(cx)), ["a.txt", "b.txt", "c.txt"]);

    // Ctrl+Shift+T: the last one closed, then the one before.
    cx.simulate_keystrokes(&secondary("shift-t"));
    assert_eq!(tab_names(&workspace, cx), ["a.txt"]);
    fs::remove_file(&b).unwrap();
    command(&workspace, "file.restore-recent-closed", cx).unwrap();
    assert_eq!(
        tab_names(&workspace, cx),
        ["a.txt", "c.txt"],
        "b.txt is gone"
    );
    take_info();
    command(&workspace, "file.restore-recent-closed", cx).unwrap();
    assert_eq!(tab_names(&workspace, cx), ["a.txt", "c.txt"]);
    assert_eq!(take_info().len(), 1, "nothing left to reopen");

    // Open All Recent Files: what exists opens, what is gone leaves the list.
    command(&workspace, "file.close-all", cx).unwrap();
    assert_eq!(names(recent(cx)).len(), 3);
    command(&workspace, "file.open-all-recent", cx).unwrap();
    let mut open_now = tab_names(&workspace, cx);
    open_now.sort();
    assert_eq!(open_now, ["a.txt", "c.txt"]);
    assert!(recent(cx).is_empty());
    // With nothing recent, nothing happens.
    command(&workspace, "file.open-all-recent", cx).unwrap();
    assert_eq!(tab_names(&workspace, cx).len(), 2);
}

#[gpui_kit::test]
fn untitled_names_are_kept_in_sessions(cx: &mut TestAppContext) {
    let backups = tempfile::tempdir().unwrap();
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("draft");
    rename(&workspace, "Plan", cx).unwrap();
    let buffer = active(&workspace, cx).read_with(cx, |view, _| view.buffer.clone());
    buffer.update(cx, |buffer, _| {
        buffer.backup = Some(crate::buffer::Backup {
            name: "Plan@1".into(),
            revision: None,
        });
    });
    let mut session = workspace.read_with(cx, |workspace, cx| workspace.session(true, cx));
    assert_eq!(session.documents[0].title.as_deref(), Some("Plan"));

    // Back from the session with its name; a blank name is "new N" again.
    fs::write(backups.path().join("Plan@1"), "draft").unwrap();
    command(&workspace, "file.close-all", cx).unwrap();
    cx.simulate_prompt_answer("Don't Save");
    cx.run_until_parked();
    session.documents.push(birchpad_config::SessionDocument {
        untitled: Some(7),
        title: Some("  ".into()),
        backup: Some("Plan@1".into()),
        ..Default::default()
    });
    session.main_view.tabs.push(birchpad_config::SessionTab {
        document: 1,
        ..Default::default()
    });
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.restore_session(&session, Some(backups.path()), window, cx)
    });
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx), ["new 1", "Plan", "new 7"]);
}

#[gpui_kit::test]
fn saving_where_the_file_system_refuses_keeps_the_changes(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("gone");
    fs::create_dir(&folder).unwrap();
    let file = write(&folder, "a.txt", "old");
    let (workspace, cx) = open_workspace(cx);
    open(&workspace, &file, cx);
    cx.simulate_input("new ");
    // Its folder is deleted by another program: the save fails and says so.
    fs::remove_dir_all(&folder).unwrap();
    take_info();
    cx.simulate_keystrokes(&secondary("s"));
    cx.run_until_parked();
    assert!(modified(&workspace, cx), "still unsaved");
    assert_eq!(active_text(&workspace, cx), "new old");
    assert!(!file.exists());
    let errors = cx.update(|window, cx| window.notifications(cx).len());
    assert!(errors >= 1, "the error is shown");

    // A backup copy that cannot be made stops the save, which leaves the file as it was.
    fs::create_dir(&folder).unwrap();
    fs::write(&file, "old").unwrap();
    let blocker = dir.path().join("not a folder");
    fs::write(&blocker, "").unwrap();
    cx.update(|_, cx| {
        let files = &mut cx.global_mut::<AppState>().settings.files;
        files.backup_on_save = birchpad_config::BackupOnSave::Verbose;
        files.backup_folder = blocker.join("copies").display().to_string();
    });
    cx.simulate_keystrokes(&secondary("s"));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&file).unwrap(), "old");
    assert!(modified(&workspace, cx));
    // Once the copy can be made, the save goes through.
    cx.update(|_, cx| {
        let files = &mut cx.global_mut::<AppState>().settings.files;
        files.backup_folder = dir.path().join("copies").display().to_string();
    });
    cx.simulate_keystrokes(&secondary("s"));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&file).unwrap(), "new old");
    assert!(!modified(&workspace, cx));
}

#[gpui_kit::test]
fn renaming_a_file_deleted_meanwhile_fails(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let file = write(dir.path(), "a.txt", "text");
    let (workspace, cx) = open_workspace(cx);
    open(&workspace, &file, cx);
    fs::remove_file(&file).unwrap();
    assert!(rename(&workspace, "b.txt", cx).is_err());
    assert_eq!(tab_names(&workspace, cx), ["a.txt"]);
    assert!(!dir.path().join("b.txt").exists());
}

#[gpui_kit::test]
fn a_recent_file_that_is_gone_leaves_the_list(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let file = write(dir.path(), "a.txt", "a");
    let (workspace, cx) = open_workspace(cx);
    open(&workspace, &file, cx);
    command(&workspace, "file.close", cx).unwrap();
    assert_eq!(recent(cx).len(), 1);
    fs::remove_file(&file).unwrap();
    let path = recent(cx)[0].clone();
    let error = run(
        &workspace,
        Invocation::with_args("file.open-recent", json!({ "path": path })),
        cx,
    )
    .unwrap_err();
    assert!(error.to_string().contains("a.txt"), "{error}");
    assert!(recent(cx).is_empty());
    assert_eq!(tab_names(&workspace, cx), ["new 1"]);
}
