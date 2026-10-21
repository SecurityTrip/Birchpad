//! Tests of Define your language: writing languages into the UDL folder, and the dialog.

use birchpad_commands::Invocation;
use gpui_kit::{TestAppContext, VisualTestContext};
use serde_json::json;

use crate::workspace::open_workspace;
use birchpad::app_state::AppState;
use birchpad::user_languages::define::*;
use birchpad_syntax::UdlDefinition;
use gpui_kit::component::WindowExt as _;
use std::path::Path;

const TWO: &str = r#"<NotepadPlus>
    <UserLang name="One" ext="one"><KeywordLists><Keywords name="Keywords1">a</Keywords></KeywordLists></UserLang>
    <UserLang name="Two" ext="two"><KeywordLists><Keywords name="Keywords1">b</Keywords></KeywordLists></UserLang>
</NotepadPlus>"#;

fn names_in(path: &Path) -> Vec<String> {
    UdlDefinition::parse_all(&std::fs::read_to_string(path).unwrap())
        .unwrap()
        .into_iter()
        .map(|language| language.name)
        .collect()
}

#[test]
fn file_names_are_safe() {
    assert_eq!(file_name("Mini"), "Mini.xml");
    assert_eq!(file_name(" a/b:c*? "), "a_b_c__.xml");
    assert_eq!(file_name("..."), "language.xml");
    assert_eq!(file_name(""), "language.xml");
}

#[test]
fn a_new_language_gets_a_file_of_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let mut language = UdlDefinition::new("Mini");
    language.set_list("Keywords1", "if");
    let path = save_definition(dir.path(), None, &language).unwrap();
    assert_eq!(path, dir.path().join("Mini.xml"));
    assert_eq!(
        find_definition(dir.path(), "Mini")
            .unwrap()
            .list("Keywords1"),
        "if"
    );
    // A file of that name holding another language: the next free name.
    std::fs::write(dir.path().join("Other.xml"), TWO).unwrap();
    let other = UdlDefinition::new("Other");
    let path = save_definition(dir.path(), None, &other).unwrap();
    assert_eq!(path, dir.path().join("Other 2.xml"));
    assert_eq!(names_in(&dir.path().join("Other.xml")), ["One", "Two"]);
}

#[test]
fn an_edited_language_takes_its_old_place() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("pair.xml"), TWO).unwrap();
    // Renamed from a file of two: the other stays there, the edited one gets its own file.
    let mut two = find_definition(dir.path(), "Two").unwrap();
    two.name = "Second".into();
    let path = save_definition(dir.path(), Some("Two"), &two).unwrap();
    assert_eq!(path, dir.path().join("Second.xml"));
    assert_eq!(names_in(&dir.path().join("pair.xml")), ["One"]);
    // Alone in its file: written in place.
    two.set_list("Keywords1", "c");
    let again = save_definition(dir.path(), Some("Second"), &two).unwrap();
    assert_eq!(again, path);
    assert_eq!(
        find_definition(dir.path(), "Second")
            .unwrap()
            .list("Keywords1"),
        "c"
    );
    assert!(find_definition(dir.path(), "Two").is_none());
}

#[test]
fn a_language_without_a_name_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let error = save_definition(dir.path(), None, &UdlDefinition::new("  ")).unwrap_err();
    assert!(error.to_string().contains("name"), "{error}");
    assert!(
        std::fs::read_dir(dir.path()).unwrap().next().is_none(),
        "nothing written"
    );
    assert!(find_definition(&dir.path().join("missing"), "x").is_none());
}

#[gpui_kit::test]
fn the_dialog_writes_the_language_and_colors_with_it(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().paths.user_settings = Some(dir.path().join("settings.toml"));
        birchpad::user_languages::init(cx);
    });
    let opened = workspace.update_in(cx, |_, window, cx| {
        open(cx.entity().downgrade(), None, window, cx)
    });
    opened.update_in(cx, |dialog, window, cx| {
        dialog.set_field("name", "Tiny", window, cx);
        dialog.set_field("extensions", "tiny", window, cx);
        dialog.set_field("Keywords1", "let", window, cx);
        dialog.set_field("line-comment", "--", window, cx);
        dialog.set_field("delimiter-0-open", "'", window, cx);
        dialog.set_field("delimiter-0-close", "'", window, cx);
    });
    let saved = opened.update_in(cx, |dialog, window, cx| dialog.save(window, cx));
    assert!(saved);
    cx.run_until_parked();
    let tiny = find_definition(&dir.path().join("userDefineLangs"), "Tiny").unwrap();
    assert_eq!(tiny.extensions, "tiny");
    assert_eq!(tiny.comments().0, "--");
    // The language menu has it, and a document can be put in it.
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.dispatch(
                &Invocation::with_args("language.set", json!({ "language": "udl:Tiny" })),
                window,
                cx,
            )
        })
        .unwrap();
    cx.update(|_, cx| assert!(birchpad::user_languages::find("Tiny", cx).is_some()));

    // Saving without a name keeps the dialog open with the reason.
    opened.update_in(cx, |dialog, window, cx| {
        dialog.set_field("name", " ", window, cx)
    });
    let saved = opened.update_in(cx, |dialog, window, cx| dialog.save(window, cx));
    assert!(!saved);
    let error = opened
        .read_with(cx, |dialog, _| dialog.error.clone())
        .unwrap();
    assert!(error.contains("name"), "{error}");
}

#[gpui_kit::test]
fn the_command_opens_the_active_documents_language(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("userDefineLangs")).unwrap();
    std::fs::write(dir.path().join("userDefineLangs").join("pair.xml"), TWO).unwrap();
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().paths.user_settings = Some(dir.path().join("settings.toml"));
        birchpad::user_languages::init(cx);
    });
    let run = |invocation: Invocation, cx: &mut VisualTestContext| {
        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.dispatch(&invocation, window, cx)
            })
            .unwrap();
        cx.run_until_parked();
    };
    run(
        Invocation::with_args("language.set", json!({ "language": "udl:Two" })),
        cx,
    );
    run(Invocation::new("language.define-udl"), cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    // The dialog shows the active document's language, with its own lists.
    let opened = workspace.update_in(cx, |workspace, window, cx| {
        workspace.define_user_language(None, window, cx)
    });
    let shown = opened.read_with(cx, |dialog, cx| dialog.definition(cx));
    assert_eq!(shown.name, "Two");
    assert_eq!(shown.list("Keywords1"), "b");
    // By name; and a name no file has opens a new language.
    let one = workspace.update_in(cx, |workspace, window, cx| {
        workspace.define_user_language(Some("One".into()), window, cx)
    });
    assert_eq!(
        one.read_with(cx, |dialog, cx| dialog.definition(cx))
            .extensions,
        "one"
    );
    let new = workspace.update_in(cx, |workspace, window, cx| {
        workspace.define_user_language(Some("Nobody".into()), window, cx)
    });
    assert_eq!(
        new.read_with(cx, |dialog, cx| dialog.definition(cx)).name,
        ""
    );
}
