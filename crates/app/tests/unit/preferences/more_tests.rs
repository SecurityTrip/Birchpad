//! Tests of the Preferences that Phase 6 adds: languages' tab settings, new documents, the
//! default folder, backups on save, the title bar, and inaccessible files in sessions.

use std::fs;
use std::path::PathBuf;

use birchpad_commands::Invocation;
use birchpad_config::{BackupOnSave, DefaultDirectory, LanguageSettings};
use birchpad_core::{Encoding, LineEnding};
use chrono::NaiveDate;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use crate::app_state::AppState;
use crate::file_ops::backup_path;
use crate::workspace::Workspace;
use crate::workspace::tests::{active_text, open_workspace, secondary, tab_names};

fn settings(cx: &mut VisualTestContext, change: impl FnOnce(&mut birchpad_config::Settings)) {
    cx.update(|_, cx| change(&mut cx.global_mut::<AppState>().settings));
}

fn run(workspace: &Entity<Workspace>, invocation: Invocation, cx: &mut VisualTestContext) {
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.dispatch(&invocation, window, cx)
        })
        .unwrap();
    cx.run_until_parked();
}

#[test]
fn language_names_as_typed() {
    crate::i18n::set_language("en");
    assert_eq!(super::language_id("python").unwrap(), "python");
    assert_eq!(super::language_id(" Python ").unwrap(), "python");
    assert_eq!(super::language_id("TEXT").unwrap(), "text");
    assert!(super::language_id("").is_err());
    assert!(super::language_id("klingon").is_err());
}

#[test]
fn where_backups_on_save_go() {
    let now = NaiveDate::from_ymd_opt(2026, 10, 13)
        .unwrap()
        .and_hms_opt(9, 5, 7)
        .unwrap();
    let file = PathBuf::from("/notes/a.txt");
    assert_eq!(backup_path(&file, BackupOnSave::None, "", now), None);
    assert_eq!(
        backup_path(&file, BackupOnSave::Simple, "/ignored", now),
        Some(PathBuf::from("/notes/a.txt.bak"))
    );
    assert_eq!(
        backup_path(&file, BackupOnSave::Verbose, "", now),
        Some(PathBuf::from(
            "/notes/nppBackup/a.txt.2026-10-13_090507.bak"
        ))
    );
    assert_eq!(
        backup_path(&file, BackupOnSave::Verbose, " /backups ", now),
        Some(PathBuf::from("/backups/a.txt.2026-10-13_090507.bak"))
    );
    assert_eq!(
        backup_path(&PathBuf::from("/"), BackupOnSave::Simple, "", now),
        None
    );
}

#[gpui_kit::test]
fn backups_are_made_before_saving(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, "old").unwrap();
    let (workspace, cx) = open_workspace(cx);
    settings(cx, |settings| {
        settings.files.backup_on_save = BackupOnSave::Simple
    });
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(&path, window, cx)
    });
    cx.run_until_parked();
    cx.simulate_input("new ");
    cx.simulate_keystrokes(&secondary("s"));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&path).unwrap(), "new old");
    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt.bak")).unwrap(),
        "old"
    );

    // Verbose, in a folder of their own.
    let folder = dir.path().join("copies");
    settings(cx, |settings| {
        settings.files.backup_on_save = BackupOnSave::Verbose;
        settings.files.backup_folder = folder.display().to_string();
    });
    cx.simulate_input("x");
    cx.simulate_keystrokes(&secondary("s"));
    cx.run_until_parked();
    let copies: Vec<String> = fs::read_dir(&folder)
        .unwrap()
        .map(|entry| fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect();
    assert_eq!(copies, ["new old"]);
    // A new file has nothing to copy.
    cx.simulate_keystrokes(&secondary("n"));
    cx.simulate_input("fresh");
    cx.simulate_keystrokes(&secondary("s"));
    cx.run_until_parked();
    let fresh = dir.path().join("fresh.txt");
    cx.simulate_new_path_selection(move |_| Some(fresh));
    cx.run_until_parked();
    assert_eq!(fs::read_dir(&folder).unwrap().count(), 1);
}

#[gpui_kit::test]
fn languages_have_their_own_tab_settings(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    settings(cx, |settings| {
        settings.languages.insert(
            "python".into(),
            LanguageSettings {
                tab_width: Some(2),
                insert_spaces: Some(true),
            },
        );
    });
    cx.simulate_keystrokes("tab");
    assert_eq!(
        active_text(&workspace, cx),
        "\t",
        "plain text: the editor's"
    );
    run(
        &workspace,
        Invocation::with_args("language.set", json!({ "language": "python" })),
        cx,
    );
    cx.simulate_keystrokes("tab");
    assert_eq!(active_text(&workspace, cx), "\t  ");
}

#[gpui_kit::test]
fn new_documents_take_their_defaults(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    settings(cx, |settings| {
        settings.new_document.encoding = "utf-16le-bom".into();
        settings.new_document.line_ending = "cr".into();
        settings.new_document.language = "rust".into();
    });
    cx.simulate_keystrokes(&secondary("n"));
    let (format, language) = workspace.read_with(cx, |workspace, cx| {
        let buffer = workspace.active_view(cx).unwrap().read(cx).buffer.read(cx);
        (buffer.doc().format(), buffer.language().map(|l| l.id))
    });
    assert_eq!(
        (format.encoding, format.bom, format.line_ending),
        (Encoding::Utf16Le, true, LineEnding::Cr)
    );
    assert_eq!(language, Some("rust"));
    // Names it does not know: UTF-8, the platform's line ending, plain text.
    settings(cx, |settings| {
        settings.new_document.encoding = "martian".into();
        settings.new_document.line_ending = "sideways".into();
        settings.new_document.language = "klingon".into();
    });
    cx.simulate_keystrokes(&secondary("n"));
    let (format, language) = workspace.read_with(cx, |workspace, cx| {
        let buffer = workspace.active_view(cx).unwrap().read(cx).buffer.read(cx);
        (buffer.doc().format(), buffer.language().map(|l| l.id))
    });
    assert_eq!(format, birchpad_core::Format::new());
    assert_eq!(language, None);
    assert_eq!(tab_names(&workspace, cx), ["new 1", "new 2", "new 3"]);
}

#[gpui_kit::test]
fn the_default_folder_and_the_title(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, "a").unwrap();
    let other = tempfile::tempdir().unwrap();
    let (workspace, cx) = open_workspace(cx);
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(&path, window, cx)
    });
    cx.run_until_parked();
    let folder = |cx: &mut VisualTestContext| {
        workspace.read_with(cx, |workspace, cx| workspace.default_directory(cx))
    };
    let parent = path.parent().unwrap().to_owned();
    assert_eq!(
        std::path::absolute(folder(cx)).unwrap(),
        std::path::absolute(&parent).unwrap()
    );
    settings(cx, |settings| {
        settings.files.default_directory = DefaultDirectory::Custom;
        settings.files.default_directory_path = other.path().display().to_string();
    });
    assert_eq!(folder(cx), other.path());
    // A folder that is not there: the document's.
    settings(cx, |settings| {
        settings.files.default_directory_path = "/no/such".into()
    });
    assert_ne!(folder(cx), PathBuf::from("/no/such"));
    // Remember: the folder last chosen.
    settings(cx, |settings| {
        settings.files.default_directory = DefaultDirectory::Remember
    });
    cx.update(|_, cx| crate::file_ops::remember_directory(&other.path().join("x.txt"), cx));
    assert_eq!(folder(cx), other.path());

    // The title: the path, or only the name.
    let title = |cx: &mut VisualTestContext| {
        workspace.read_with(cx, |workspace, cx| workspace.window_title(cx))
    };
    assert!(title(cx).contains(&parent.display().to_string()));
    settings(cx, |settings| {
        settings.appearance.title_file_name_only = true
    });
    assert_eq!(title(cx), "a.txt - Birchpad");
    cx.simulate_input("!");
    assert_eq!(title(cx), "*a.txt - Birchpad");
}

#[gpui_kit::test]
fn inaccessible_files_stay_in_the_session(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let here = dir.path().join("here.txt");
    fs::write(&here, "here").unwrap();
    let away = dir.path().join("away.txt");
    let mut session = birchpad_config::Session::new();
    for path in [&here, &away] {
        session.documents.push(birchpad_config::SessionDocument {
            path: Some(path.clone()),
            ..Default::default()
        });
        session.main_view.tabs.push(birchpad_config::SessionTab {
            document: session.documents.len() - 1,
            ..Default::default()
        });
    }
    let (workspace, cx) = open_workspace(cx);
    let restore = |session: &birchpad_config::Session, cx: &mut VisualTestContext| {
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.restore_session(session, None, window, cx)
        });
        cx.run_until_parked();
        workspace.read_with(cx, |workspace, cx| workspace.session(false, cx))
    };
    // Off: the missing file is forgotten.
    let written = restore(&session, cx);
    assert_eq!(written.documents.len(), 1);
    // On: it stays, at the end.
    settings(cx, |settings| settings.session.remember_inaccessible = true);
    let written = restore(&session, cx);
    let paths: Vec<_> = written
        .documents
        .iter()
        .filter_map(|document| document.path.clone())
        .collect();
    assert!(paths.contains(&away), "{paths:?}");
    assert_eq!(written.main_view.tabs.len(), 2);
}
