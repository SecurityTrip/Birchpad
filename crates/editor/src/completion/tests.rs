use birchpad_commands::Invocation;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use super::*;
use crate::test_support::Workspace;
use crate::test_support::{active_text, secondary};

/// A window with auto-completion as Birchpad starts with it.
fn open_workspace(cx: &mut TestAppContext) -> (Entity<Workspace>, &mut VisualTestContext) {
    let (workspace, cx) = crate::test_support::open_workspace(cx);
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().settings.completion =
            birchpad_config::CompletionSettings::default();
    });
    (workspace, cx)
}

fn view(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<EditorView> {
    workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap())
}

fn run(workspace: &Entity<Workspace>, invocation: Invocation, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&invocation, window, cx).unwrap()
    });
    cx.run_until_parked();
}

fn labels(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> Option<Vec<String>> {
    view.read_with(cx, |view, _| {
        view.completion
            .as_ref()
            .map(|popup| popup.items.iter().map(|item| item.label.clone()).collect())
    })
}

#[gpui_kit::test]
fn brackets_and_quotes_close_themselves(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("f(");
    assert_eq!(active_text(&workspace, cx), "f()");
    cx.simulate_input("x)");
    assert_eq!(active_text(&workspace, cx), "f(x)");
    cx.simulate_input(" \"");
    assert_eq!(active_text(&workspace, cx), "f(x) \"\"");
    cx.simulate_keystrokes("backspace");
    assert_eq!(active_text(&workspace, cx), "f(x) ");
    // Turned off: brackets are typed alone.
    cx.update(|_, cx| {
        cx.global_mut::<AppState>()
            .settings
            .completion
            .auto_close
            .clear()
    });
    cx.simulate_input("[");
    assert_eq!(active_text(&workspace, cx), "f(x) [");
    // Overwrite mode types over the text instead.
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().settings.completion.auto_close = "()".into();
    });
    run(&workspace, Invocation::new("edit.toggle-overwrite"), cx);
    cx.simulate_input("(");
    assert_eq!(active_text(&workspace, cx), "f(x) [(");
}

#[gpui_kit::test]
fn words_of_the_document_are_offered_while_typing(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let view = view(&workspace, cx);
    run(
        &workspace,
        Invocation::with_args("edit.insert-text", json!({ "text": "printer print_all\n" })),
        cx,
    );
    cx.simulate_input("pr");
    assert_eq!(
        labels(&view, cx),
        Some(vec!["print_all".to_owned(), "printer".to_owned()])
    );
    // The keys move in the list and take the entry.
    cx.simulate_keystrokes("down");
    cx.simulate_keystrokes("enter");
    assert_eq!(active_text(&workspace, cx), "printer print_all\nprinter");
    assert_eq!(labels(&view, cx), None);
    // A character that is not of a word closes it, as does Escape.
    cx.simulate_input(" pri");
    assert!(labels(&view, cx).is_some());
    cx.simulate_keystrokes("escape");
    assert_eq!(labels(&view, cx), None);
    cx.simulate_input("n");
    assert!(labels(&view, cx).is_some());
    cx.simulate_input(".");
    assert_eq!(labels(&view, cx), None);
    // Moving the caret closes it.
    cx.simulate_input("pr");
    assert!(labels(&view, cx).is_some());
    run(&workspace, Invocation::new("cursor.document-start"), cx);
    view.update_in(cx, |view, window, cx| {
        view.run_command(
            &birchpad_ui::RunCommand(Invocation::new("cursor.document-end")),
            window,
            cx,
        );
    });
    assert_eq!(labels(&view, cx), None);
}

#[gpui_kit::test]
fn settings_and_the_key_to_ask(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let view = view(&workspace, cx);
    run(
        &workspace,
        Invocation::with_args("edit.insert-text", json!({ "text": "alpha alpine\n" })),
        cx,
    );
    cx.update(|_, cx| {
        let completion = &mut cx.global_mut::<AppState>().settings.completion;
        completion.min_chars = 3;
    });
    cx.simulate_input("al");
    assert_eq!(labels(&view, cx), None, "under the minimum");
    cx.simulate_input("p");
    assert!(labels(&view, cx).is_some());
    cx.simulate_keystrokes("escape");
    cx.update(|_, cx| cx.global_mut::<AppState>().settings.completion.words = false);
    cx.simulate_input("h");
    assert_eq!(labels(&view, cx), None, "turned off");
    // Ctrl+Space asks for the list anyway.
    cx.simulate_keystrokes("backspace");
    cx.simulate_keystrokes(&secondary("space"));
    assert_eq!(
        labels(&view, cx),
        Some(vec!["alpha".to_owned(), "alpine".to_owned()])
    );
    // Backspace keeps the list in step; deleting the word closes it.
    cx.simulate_keystrokes("backspace backspace backspace");
    assert_eq!(labels(&view, cx), None);
}

#[gpui_kit::test]
fn api_files_add_keywords_and_call_tips(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("autoCompletion")).unwrap();
    std::fs::write(
        dir.path().join("autoCompletion").join("python.xml"),
        r#"<NotepadPlus><AutoComplete><KeyWord name="print" func="yes"><Overload retVal="None" descr="Prints."><Param name="*values"/></Overload></KeyWord><KeyWord name="pass"/></AutoComplete></NotepadPlus>"#,
    )
    .unwrap();
    forget_api_files();
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().paths.user_settings = Some(dir.path().join("settings.toml"));
    });
    run(
        &workspace,
        Invocation::with_args("language.set", json!({ "language": "python" })),
        cx,
    );
    let view = view(&workspace, cx);
    cx.simulate_input("p");
    let items = view.read_with(cx, |view, _| view.completion.clone().unwrap().items);
    assert_eq!(items[0].label, "pass");
    assert_eq!(items[1].detail.as_deref(), Some("None print(*values)"));
    cx.simulate_keystrokes("escape");
    cx.simulate_input("rint(");
    let tip = view.read_with(cx, |view, _| view.call_tip.clone()).unwrap();
    assert_eq!(tip.text, "None print(*values)\nPrints.");
    // The closing bracket steps over its partner and closes the tip.
    cx.simulate_input("x)");
    assert_eq!(view.read_with(cx, |view, _| view.call_tip.clone()), None);
    assert_eq!(active_text(&workspace, cx), "print(x)");
    // Ctrl+Shift+Space shows it again inside the call; Escape closes it.
    cx.simulate_keystrokes("left");
    cx.simulate_keystrokes(&secondary("shift-space"));
    assert!(view.read_with(cx, |view, _| view.call_tip.is_some()));
    cx.simulate_keystrokes("escape");
    assert!(view.read_with(cx, |view, _| view.call_tip.is_none()));
    forget_api_files();
}

#[gpui_kit::test]
fn a_broken_or_missing_api_file_is_no_api(cx: &mut TestAppContext) {
    let (_, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("autoCompletion")).unwrap();
    std::fs::write(
        dir.path().join("autoCompletion").join("rust.xml"),
        "<broken",
    )
    .unwrap();
    forget_api_files();
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().paths.user_settings = Some(dir.path().join("settings.toml"));
        assert!(api_file(Some("rust"), cx).is_none());
        assert!(api_file(Some("lua"), cx).is_none());
        assert!(api_file(None, cx).is_none());
    });
    forget_api_files();
}

#[test]
fn items_put_the_api_first_and_each_word_once() {
    let api = ApiFile::parse(
        r#"<NotepadPlus><AutoComplete><KeyWord name="print"/></AutoComplete></NotepadPlus>"#,
    )
    .unwrap();
    let text = birchpad_core::Rope::from_str("print printer pr");
    let found = items(Some(&api), &text, 14, 16);
    let labels: Vec<_> = found.iter().map(|item| item.label.as_str()).collect();
    assert_eq!(labels, ["print", "printer"]);
    assert!(items(None, &text, 16, 16).is_empty());
}
