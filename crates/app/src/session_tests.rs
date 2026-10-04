//! Tests of sessions: quitting with unsaved documents and getting everything back, periodic
//! backups after a crash, quitting without backups, `-nosession`, and session files.

use std::fs;
use std::path::Path;

use birchpad_commands::Invocation;
use birchpad_config::{ConfigPaths, Session, Settings, Sources, resolve};
use birchpad_core::{Range, Selection};
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use crate::app_state::AppState;
use crate::editor::EditorView;
use crate::workspace::Workspace;
use crate::workspace::tests::{active_text, document_start, pane_views, panes, secondary};

/// The application's globals, with its data in `data`.
fn set_up(cx: &mut TestAppContext, data: &Path, configure: impl FnOnce(&mut Settings)) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        let mut settings = resolve(Sources::default());
        configure(&mut settings.settings);
        let paths = ConfigPaths {
            user_data: Some(data.to_owned()),
            ..ConfigPaths::default()
        };
        cx.set_global(AppState::new(settings, paths));
        crate::commands::init(None, cx);
    });
}

/// A window as a launch opens it: with the last session, or "new 1".
fn launch(cx: &mut TestAppContext) -> (Entity<Workspace>, &'static mut VisualTestContext) {
    let (window, workspace) = cx.update(|cx| {
        let options = gpui_kit::WindowOptions {
            window_bounds: Some(gpui_kit::WindowBounds::Windowed(
                gpui_kit::Bounds::maximized(None, cx),
            )),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| {
                let mut workspace = Workspace::new(window, cx);
                workspace.restore_last_session(window, cx);
                workspace
            })
        })
        .expect("open test window")
    });
    let cx = VisualTestContext::from_window(window, cx).into_mut();
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    (workspace, cx)
}

fn run(workspace: &Entity<Workspace>, invocation: Invocation, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&invocation, window, cx).unwrap();
    });
    cx.run_until_parked();
}

fn open(workspace: &Entity<Workspace>, path: &Path, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(path, window, cx);
    });
    cx.run_until_parked();
}

fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

/// Text, modified flag, bookmarks, chosen language and selection of a view.
fn describe(
    view: &Entity<EditorView>,
    cx: &mut VisualTestContext,
) -> (String, bool, Vec<usize>, Option<&'static str>, Range) {
    view.read_with(cx, |view, cx| {
        let buffer = view.buffer.read(cx);
        let language = buffer.language().map(|language| language.id);
        (
            buffer.doc().text().to_string(),
            buffer.is_modified(),
            buffer.bookmark_lines(),
            language,
            view.selection.primary(),
        )
    })
}

fn backup_files(data: &Path) -> Vec<String> {
    let mut files: Vec<String> = fs::read_dir(data.join("backup"))
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

#[gpui_kit::test]
fn quitting_keeps_unsaved_documents_and_where_they_were(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let file = data.path().join("main.rs");
    fs::write(&file, "fn main() {\n    let x = 1;\n}\n").unwrap();
    set_up(cx, data.path(), |_| {});
    let (workspace, first) = launch(cx);

    // An untitled draft in Python, and a file with unsaved changes, a bookmark, a collapsed
    // fold and a caret, also shown in the second view.
    first.simulate_input("draft = 1");
    run(
        &workspace,
        Invocation::with_args("language.set", json!({ "language": "python" })),
        first,
    );
    open(&workspace, &file, first);
    first.simulate_keystrokes(document_start());
    first.simulate_input("// edit\n");
    first.simulate_keystrokes("down ctrl-f2");
    run(&workspace, Invocation::new("view.fold-all"), first);
    let edited = pane_views(&workspace, 0, first)[1].clone();
    edited.update(first, |view, cx| {
        view.selection = Selection::single(Range::new(9, 11));
        cx.notify();
    });
    run(
        &workspace,
        Invocation::new("view.clone-to-other-view"),
        first,
    );
    let before = describe(&edited, first);
    let collapsed = edited.read_with(first, |view, cx| view.collapsed_lines(cx));
    assert_eq!(collapsed, [1]);

    // Quitting does not ask: the unsaved text goes to backup copies.
    workspace.update_in(first, |workspace, window, cx| {
        assert!(workspace.can_quit_now(cx));
        drop(workspace.prepare_to_quit(window, cx));
    });
    assert!(!first.has_pending_prompt());
    assert_eq!(backup_files(data.path()).len(), 2);
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "fn main() {\n    let x = 1;\n}\n"
    );

    // The next launch puts everything back.
    let (workspace, second) = launch(cx);
    second.run_until_parked();
    assert_eq!(
        panes(&workspace, second),
        ([names(&["new 1", "main.rs"]), names(&["main.rs"])], 1)
    );
    let [draft, restored] = [0, 1].map(|index| pane_views(&workspace, 0, second)[index].clone());
    let (text, modified, _, language, _) = describe(&draft, second);
    assert_eq!(
        (text.as_str(), modified, language),
        ("draft = 1", true, Some("python"))
    );
    assert_eq!(describe(&restored, second), before);
    let collapsed = restored.read_with(second, |view, cx| view.collapsed_lines(cx));
    assert_eq!(collapsed, [1], "the fold stays collapsed once parsed");
    let clone = pane_views(&workspace, 1, second)[0].clone();
    let same_buffer = clone.read_with(second, |clone, cx| clone.buffer == restored.read(cx).buffer);
    assert!(same_buffer, "the clone is a view of the same document");
}

#[gpui_kit::test]
fn backups_after_a_crash_write_only_what_changed(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    set_up(cx, data.path(), |_| {});
    let (workspace, first) = launch(cx);
    let snapshot = |cx: &mut VisualTestContext| {
        let writing = workspace.update(cx, |workspace, cx| workspace.write_snapshot_later(cx));
        cx.run_until_parked();
        drop(writing);
        cx.run_until_parked();
    };
    first.simulate_input("one");
    snapshot(first);
    let files = backup_files(data.path());
    assert_eq!(files.len(), 1);
    let backup = data.path().join("backup").join(&files[0]);
    assert_eq!(fs::read_to_string(&backup).unwrap(), "one");

    // Unchanged: nothing is written again.
    fs::remove_file(&backup).unwrap();
    fs::remove_file(data.path().join("session.toml")).unwrap();
    snapshot(first);
    assert!(!backup.exists() && !data.path().join("session.toml").exists());

    first.simulate_input(" two");
    snapshot(first);
    assert_eq!(fs::read_to_string(&backup).unwrap(), "one two");

    // The process dies without quitting: the last backup comes back.
    let (workspace, second) = launch(cx);
    assert_eq!(panes(&workspace, second).0[0], names(&["new 1"]));
    assert_eq!(active_text(&workspace, second), "one two");
}

#[gpui_kit::test]
fn without_backups_quitting_asks_and_remembers_the_files(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let file = data.path().join("notes.txt");
    fs::write(&file, "notes").unwrap();
    set_up(cx, data.path(), |settings| {
        settings.session.backup_unsaved = false;
    });
    let (workspace, first) = launch(cx);
    first.simulate_input("draft");
    open(&workspace, &file, first);
    first.simulate_keystrokes("end");
    first.simulate_input("!");

    let quitting = workspace.update_in(first, |workspace, window, cx| {
        assert!(!workspace.can_quit_now(cx));
        workspace.prepare_to_quit(window, cx)
    });
    first.run_until_parked();
    for _ in 0..2 {
        assert!(first.has_pending_prompt());
        first.simulate_prompt_answer("Don't Save");
        first.run_until_parked();
    }
    drop(quitting);
    assert!(backup_files(data.path()).is_empty());

    // The file reopens as it is on disk, at the caret; the untitled draft is gone.
    let (workspace, second) = launch(cx);
    assert_eq!(panes(&workspace, second).0[0], names(&["notes.txt"]));
    let view = pane_views(&workspace, 0, second)[0].clone();
    let (text, modified, _, _, caret) = describe(&view, second);
    assert_eq!(
        (text.as_str(), modified, caret),
        ("notes", false, Range::point(5))
    );
}

#[gpui_kit::test]
fn no_session_neither_restores_nor_saves(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let file = data.path().join("a.txt");
    fs::write(&file, "a").unwrap();
    let mut session = Session::new();
    session.documents.push(birchpad_config::SessionDocument {
        path: Some(file.clone()),
        ..Default::default()
    });
    session
        .main_view
        .tabs
        .push(birchpad_config::SessionTab::default());
    session.save(&data.path().join("session.toml")).unwrap();
    let saved = fs::read_to_string(data.path().join("session.toml")).unwrap();

    set_up(cx, data.path(), |_| {});
    cx.update(|cx| cx.global_mut::<AppState>().no_session = true);
    let (workspace, first) = launch(cx);
    assert_eq!(panes(&workspace, first).0[0], names(&["new 1"]));
    first.simulate_input("text");
    let quitting = workspace.update_in(first, |workspace, window, cx| {
        workspace.prepare_to_quit(window, cx)
    });
    first.run_until_parked();
    assert!(first.has_pending_prompt(), "nothing is backed up");
    first.simulate_prompt_answer("Don't Save");
    first.run_until_parked();
    drop(quitting);
    assert_eq!(
        fs::read_to_string(data.path().join("session.toml")).unwrap(),
        saved
    );
}

#[gpui_kit::test]
fn session_files_open_alongside(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let file = data.path().join("b.txt");
    fs::write(&file, "line one\nline two\n").unwrap();
    let npp_backup = data.path().join("new 2@2024-01-01_120000");
    fs::write(&npp_backup, "from Notepad++").unwrap();
    let xml = format!(
        r#"<NotepadPlus><Session activeView="0">
            <mainView activeIndex="0">
                <File startPos="9" endPos="13" firstVisibleLine="0" xOffset="0" lang="Normal text" filename="{}" backupFilePath="">
                    <Mark line="1" />
                </File>
                <File startPos="0" endPos="0" lang="Normal text" filename="new 2" backupFilePath="{}" />
            </mainView>
            <subView activeIndex="0" />
        </Session></NotepadPlus>"#,
        file.display(),
        npp_backup.display()
    );
    let npp_session = data.path().join("session.xml");
    fs::write(&npp_session, xml).unwrap();
    set_up(cx, data.path(), |_| {});
    let (workspace, first) = launch(cx);
    workspace.update_in(first, |workspace, window, cx| {
        workspace
            .load_session_file(&npp_session, window, cx)
            .unwrap();
    });
    first.run_until_parked();
    assert_eq!(
        panes(&workspace, first).0[0],
        names(&["new 1", "b.txt", "new 2"])
    );
    let views = pane_views(&workspace, 0, first);
    let (_, _, bookmarks, _, selection) = describe(&views[1], first);
    assert_eq!((bookmarks, selection), (vec![1], Range::new(9, 13)));
    let (text, modified, ..) = describe(&views[2], first);
    assert_eq!((text.as_str(), modified), ("from Notepad++", true));

    // Save Session writes the files only; loading it into the next run opens them.
    let session = workspace.read_with(first, |workspace, cx| workspace.session(false, cx));
    assert_eq!(
        session.documents.len(),
        1,
        "untitled documents are left out"
    );
    let saved = data.path().join("saved.toml");
    birchpad_config::write_private(&saved, session.to_toml().as_bytes()).unwrap();
    first.simulate_keystrokes(&secondary("n"));
    workspace.update_in(first, |workspace, window, cx| {
        workspace.load_session_file(&saved, window, cx).unwrap();
    });
    assert_eq!(
        panes(&workspace, first).0[0].len(),
        4,
        "an open file is not opened twice"
    );
}
