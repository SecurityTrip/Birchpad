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
    assert_eq!(tip.text(), "None print(*values)\nPrints.");
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
    let all = Sources::all(&CompletionSettings::default());
    let found = items(Some(&api), &text, 14, 16, all);
    let labels: Vec<_> = found.iter().map(|item| item.label.as_str()).collect();
    assert_eq!(labels, ["print", "printer"]);
    assert!(items(None, &text, 16, 16, all).is_empty());
}

fn item_labels(found: Vec<Item>) -> Vec<String> {
    found.into_iter().map(|item| item.label).collect()
}

#[test]
fn each_source_from_its_own_number_of_characters() {
    let api = ApiFile::parse(
        r#"<NotepadPlus><AutoComplete ignoreCase="yes"><KeyWord name="Print"/><KeyWord name="1st"/></AutoComplete></NotepadPlus>"#,
    )
    .unwrap();
    let settings = CompletionSettings {
        api_min_chars: 3,
        min_chars: 2,
        ..CompletionSettings::default()
    };
    let text = birchpad_core::Rope::from_str("printer pr");
    // Two characters: the words, not yet the API.
    let found = items(Some(&api), &text, 8, 10, Sources::for_typed(&settings, 2));
    assert_eq!(item_labels(found), ["printer"]);
    // One character: nothing; three: both.
    assert!(!Sources::for_typed(&settings, 1).api);
    assert!(!Sources::for_typed(&settings, 1).words);
    let three = Sources::for_typed(&settings, 3);
    assert!(three.api && three.words);
    // A source turned off stays off, whatever the length; Ctrl+Space asks for everything.
    let no_api = CompletionSettings {
        api: false,
        ..settings.clone()
    };
    assert!(!Sources::for_typed(&no_api, 9).api);
    assert!(Sources::all(&no_api).api);
}

#[test]
fn case_and_numbers_as_the_options_say() {
    let api = ApiFile::parse(
        r#"<NotepadPlus><AutoComplete ignoreCase="yes"><KeyWord name="Print"/><KeyWord name="1st"/></AutoComplete></NotepadPlus>"#,
    )
    .unwrap();
    let text = birchpad_core::Rope::from_str("printer 1000 pr 1");
    let mut settings = CompletionSettings::default();
    let all = |settings: &CompletionSettings| Sources::all(settings);
    assert_eq!(
        item_labels(items(Some(&api), &text, 13, 15, all(&settings))),
        ["Print", "printer"]
    );
    settings.ignore_case = false;
    assert_eq!(
        item_labels(items(Some(&api), &text, 13, 15, all(&settings))),
        ["printer"],
        "Print does not start with pr"
    );
    assert!(item_labels(items(Some(&api), &text, 16, 17, all(&settings))).is_empty());
    settings.ignore_numbers = false;
    assert_eq!(
        item_labels(items(Some(&api), &text, 16, 17, all(&settings))),
        ["1st", "1000"]
    );
}

#[test]
fn a_call_tip_shows_one_overload_at_a_time() {
    let mut tip = CallTip {
        open: 0,
        overloads: vec!["f(a)".into(), "f(a, b)\nTwo.".into()],
        current: 0,
    };
    assert_eq!(tip.text(), "\u{25B2} 1 of 2 \u{25BC} f(a)");
    tip.cycle(1);
    assert_eq!(tip.text(), "\u{25B2} 2 of 2 \u{25BC} f(a, b)\nTwo.");
    tip.cycle(1);
    assert_eq!(tip.current, 0, "around the end");
    tip.cycle(-1);
    assert_eq!(tip.current, 1, "around the start");
    let single = CallTip {
        open: 0,
        overloads: vec!["g()".into()],
        current: 0,
    };
    assert_eq!(single.text(), "g()", "one overload has no counter");
    let mut none = CallTip {
        open: 0,
        overloads: Vec::new(),
        current: 0,
    };
    none.cycle(1);
    assert_eq!(none.text(), "");
}

#[gpui_kit::test]
fn tags_close_when_the_setting_says_so(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    run(
        &workspace,
        Invocation::with_args("language.set", json!({ "language": "html" })),
        cx,
    );
    cx.simulate_input("<p>");
    assert_eq!(
        active_text(&workspace, cx),
        "<p>",
        "off by default, as in Notepad++"
    );
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().settings.completion.close_tags = true;
    });
    cx.simulate_input("<b class=\"x\">");
    cx.simulate_input("bold");
    assert_eq!(active_text(&workspace, cx), "<p><b class=\"x\">bold</b>");
    cx.simulate_input("<br>");
    assert_eq!(
        active_text(&workspace, cx),
        "<p><b class=\"x\">bold<br></b>",
        "a void element"
    );
    // Plain text closes nothing.
    run(
        &workspace,
        Invocation::with_args("language.set", json!({ "language": "text" })),
        cx,
    );
    cx.simulate_input("<i>");
    assert!(active_text(&workspace, cx).ends_with("<br><i></b>"));
}

#[gpui_kit::test]
fn up_and_down_go_through_overloads(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("autoCompletion")).unwrap();
    std::fs::write(
        dir.path().join("autoCompletion").join("javascript.xml"),
        r#"<NotepadPlus><AutoComplete><KeyWord name="max" func="yes"><Overload retVal="number"><Param name="a"/></Overload><Overload retVal="number"><Param name="a"/><Param name="b"/></Overload></KeyWord></AutoComplete></NotepadPlus>"#,
    )
    .unwrap();
    forget_api_files();
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().paths.user_settings = Some(dir.path().join("settings.toml"));
    });
    // JavaScript inside HTML has JavaScript's API file.
    run(
        &workspace,
        Invocation::with_args("language.set", json!({ "language": "html" })),
        cx,
    );
    cx.simulate_input("<script>\n");
    cx.simulate_input("</script>");
    cx.simulate_keystrokes("up end");
    cx.simulate_input("max(");
    cx.run_until_parked();
    let view = view(&workspace, cx);
    let tip = |cx: &mut VisualTestContext| view.read_with(cx, |view, _| view.call_tip.clone());
    let current = tip(cx).expect("a call tip in the script").current;
    assert_eq!(current, 0);
    cx.simulate_keystrokes("down");
    assert_eq!(tip(cx).unwrap().current, 1);
    cx.simulate_keystrokes("down");
    assert_eq!(tip(cx).unwrap().current, 0, "around the end");
    cx.simulate_keystrokes("up");
    assert_eq!(tip(cx).unwrap().current, 1);
    assert!(
        active_text(&workspace, cx).contains("max()"),
        "the keys moved no caret"
    );
    forget_api_files();
}

#[test]
fn every_bundled_api_file_reads() {
    for (language, _) in BUNDLED {
        let api = bundled_api(language).unwrap_or_else(|| panic!("{language}.xml reads"));
        assert!(
            api.keywords.len() >= 20,
            "{language}: {}",
            api.keywords.len()
        );
        assert_eq!((api.start_func, api.stop_func), ('(', ')'), "{language}");
        assert!(
            birchpad_syntax::by_id(language).is_some(),
            "{language} is a language"
        );
    }
    let python = bundled_api("python").unwrap();
    let print = python.function("print").expect("print is a function");
    assert!(
        print.overloads[0]
            .params
            .iter()
            .any(|param| param == "*values")
    );
    assert!(bundled_api("lua").is_none());
    assert!(bundled_api("").is_none());
}

#[gpui_kit::test]
fn a_users_api_file_takes_the_place_of_birchpads(cx: &mut TestAppContext) {
    let (_, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("autoCompletion")).unwrap();
    std::fs::write(
        dir.path().join("autoCompletion").join("python.xml"),
        r#"<NotepadPlus><AutoComplete><KeyWord name="only"/></AutoComplete></NotepadPlus>"#,
    )
    .unwrap();
    forget_api_files();
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().paths.user_settings = Some(dir.path().join("settings.toml"));
        let python = api_file(Some("python"), cx).unwrap();
        assert_eq!(python.keywords.len(), 1, "the user's own");
        let rust = api_file(Some("rust"), cx).expect("Birchpad's");
        assert!(rust.function("println!").is_some());
    });
    forget_api_files();
}
