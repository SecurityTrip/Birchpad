//! Preferences left from Notepad++ 8 for files, search and typing (roadmap 7.4c).

use std::path::Path;

use birchpad::app_state::AppState;
use birchpad::find::find_field_fill;
use birchpad::menus::recent_label;
use birchpad::search_results::entry_per_match;
use birchpad::workspace::{Workspace, folder_files};
use birchpad_commands::Invocation;
use birchpad_config::{FileSettings, SearchingSettings, Settings};
use birchpad_core::Rope;
use birchpad_core::search::LineHit;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use crate::workspace::{active_text, open_workspace, tab_names};

fn settings(cx: &mut VisualTestContext, change: impl FnOnce(&mut Settings)) {
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
fn the_find_field_takes_the_selection_or_the_word() {
    let searching = SearchingSettings::default();
    let text = Rope::from_str("hello world");
    assert_eq!(
        find_field_fill("", &text, 2, &searching),
        (Some("hello".into()), Some(false))
    );
    assert_eq!(
        find_field_fill("wor", &text, 9, &searching),
        (Some("wor".into()), Some(false))
    );
    // Several lines: In selection, the field left alone.
    assert_eq!(
        find_field_fill("a\nb", &text, 0, &searching),
        (None, Some(true))
    );
    // Long selections: In selection from the threshold, the field up to its limit.
    let long = "x".repeat(1024);
    assert_eq!(
        find_field_fill(&long, &text, 0, &searching),
        (Some(long.clone()), Some(true))
    );
    let longer = "x".repeat(1025);
    assert_eq!(
        find_field_fill(&longer, &text, 0, &searching),
        (None, Some(true))
    );
    let never = SearchingSettings {
        in_selection_from: 0,
        fill_from_selection: 0,
        fill_from_word: false,
        ..SearchingSettings::default()
    };
    assert_eq!(
        find_field_fill("a\nb", &text, 0, &never),
        (None, Some(false))
    );
    assert_eq!(
        find_field_fill("wor", &text, 0, &never),
        (None, Some(false))
    );
    assert_eq!(find_field_fill("", &text, 2, &never), (None, Some(false)));
    // At a space, no word.
    let spaced = Rope::from_str("a  b");
    assert_eq!(
        find_field_fill("", &spaced, 2, &searching),
        (None, Some(false))
    );
}

#[test]
fn recent_files_show_by_name_or_cut_in_the_middle() {
    let path = Path::new("/home/me/projects/notes/today.txt");
    let files = |change: fn(&mut FileSettings)| {
        let mut files = FileSettings::default();
        change(&mut files);
        files
    };
    assert_eq!(
        recent_label(path, &files(|_| {})),
        path.display().to_string()
    );
    assert_eq!(
        recent_label(path, &files(|files| files.recent_file_name_only = true)),
        "today.txt"
    );
    let cut = recent_label(path, &files(|files| files.recent_max_length = 11));
    assert_eq!(cut.chars().count(), 11);
    assert!(cut.contains('…'));
    assert!(cut.ends_with("txt"));
    // Short enough: whole.
    assert_eq!(
        recent_label(
            Path::new("a.txt"),
            &files(|files| files.recent_max_length = 11)
        ),
        "a.txt"
    );
}

#[test]
fn search_results_can_list_each_match_on_its_own() {
    let line = LineHit {
        line: 3,
        text: "foo foo".into(),
        highlights: vec![0..3, 4..7],
        target: 0..3,
        matches: 2,
    };
    let single = LineHit {
        line: 4,
        text: "foo".into(),
        highlights: vec![0..3],
        target: 0..3,
        matches: 1,
    };
    let split = entry_per_match(vec![line, single.clone()]);
    assert_eq!(split.len(), 3);
    assert_eq!(split[1].target, 4..7);
    assert_eq!(split[1].matches, 1);
    assert_eq!(split[2], single);
    assert!(entry_per_match(Vec::new()).is_empty());
}

#[test]
fn a_dropped_folder_gives_its_files() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    std::fs::write(dir.path().join("a.txt"), "").unwrap();
    std::fs::write(dir.path().join("sub").join("b.txt"), "").unwrap();
    assert_eq!(folder_files(dir.path()).len(), 2);
    assert!(folder_files(&dir.path().join("missing")).is_empty());
}

#[gpui_kit::test]
fn an_untitled_tab_takes_its_first_line_as_name(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("  Shopping list\nmilk");
    let name = |cx: &mut VisualTestContext| {
        workspace.read_with(cx, |workspace, cx| {
            let view = workspace.active_view(cx).unwrap();
            birchpad::pane::tab_name(view.read(cx).buffer.read(cx), cx)
        })
    };
    assert_eq!(name(cx), "new 1");
    settings(cx, |settings| settings.new_document.first_line_name = true);
    assert_eq!(name(cx), "Shopping list");
    // An empty first line keeps the name.
    run(&workspace, Invocation::new("file.new"), cx);
    cx.simulate_input("\nsecond");
    assert_eq!(name(cx), "new 2");
}

#[gpui_kit::test]
fn a_new_document_at_startup_besides_the_others(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let start = |cx: &mut VisualTestContext| {
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_startup_document(window, cx)
        });
        cx.run_until_parked();
    };
    start(cx);
    assert_eq!(tab_names(&workspace, cx), ["new 1"], "off by default");
    settings(cx, |settings| settings.new_document.at_startup = true);
    start(cx);
    assert_eq!(
        tab_names(&workspace, cx),
        ["new 1"],
        "the empty one is enough"
    );
    cx.simulate_input("text");
    start(cx);
    assert_eq!(tab_names(&workspace, cx), ["new 1", "new 2"]);
}

#[gpui_kit::test]
fn backspace_in_the_indentation_unindents_when_asked(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("        x");
    cx.simulate_keystrokes("left");
    cx.simulate_keystrokes("backspace");
    assert_eq!(
        active_text(&workspace, cx),
        "       x",
        "one space by default"
    );
    cx.simulate_input(" ");
    settings(cx, |settings| settings.editor.backspace_unindents = true);
    cx.simulate_keystrokes("backspace");
    assert_eq!(active_text(&workspace, cx), "    x");
    // Past the indentation, Backspace deletes as always.
    cx.simulate_keystrokes("end backspace");
    assert_eq!(active_text(&workspace, cx), "    ");
}

#[gpui_kit::test]
fn enter_types_a_line_break_when_it_does_not_take_the_entry(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    settings(cx, |settings| settings.completion.accept_with_enter = false);
    cx.simulate_input("alpha a");
    run(&workspace, Invocation::new("completion.words"), cx);
    let open = |cx: &mut VisualTestContext| {
        workspace.read_with(cx, |workspace, cx| {
            workspace
                .active_view(cx)
                .unwrap()
                .read(cx)
                .completion
                .is_some()
        })
    };
    assert!(open(cx));
    run(
        &workspace,
        Invocation::with_args("completion.accept", json!({ "key": "enter" })),
        cx,
    );
    assert!(!open(cx));
    let text = |cx: &mut VisualTestContext| active_text(&workspace, cx).replace("\r\n", "\n");
    assert_eq!(text(cx), "alpha a\n");
    // Tab still takes it.
    cx.simulate_input("al");
    run(&workspace, Invocation::new("completion.words"), cx);
    run(
        &workspace,
        Invocation::with_args("completion.accept", json!({ "key": "tab" })),
        cx,
    );
    assert_eq!(text(cx), "alpha a\nalpha");
}

#[gpui_kit::test]
fn a_session_extension_opens_the_file_as_a_session(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let note = dir.path().join("note.txt");
    std::fs::write(&note, "kept").unwrap();
    let session = dir.path().join("work.npss");
    std::fs::write(
        &session,
        format!(
            "<NotepadPlus><Session activeView=\"0\"><mainView activeIndex=\"0\"><File filename=\"{}\"/></mainView></Session></NotepadPlus>",
            note.display()
        ),
    )
    .unwrap();
    let (workspace, cx) = open_workspace(cx);
    settings(cx, |settings| {
        settings.session.session_extension = ".npss".into()
    });
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(&session, window, cx)
    });
    cx.run_until_parked();
    assert!(tab_names(&workspace, cx).contains(&"note.txt".to_owned()));
    assert!(!tab_names(&workspace, cx).contains(&"work.npss".to_owned()));
}
