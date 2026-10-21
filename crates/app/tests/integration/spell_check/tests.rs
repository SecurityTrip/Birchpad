use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use crate::workspace::{active_text, open_workspace};
use anyhow::Result;
use birchpad::app_state::AppState;
use birchpad::spell_check::*;
use birchpad::workspace::Workspace;
use birchpad_commands::Invocation;

/// "walk" with -s, -ed and -ing, "the", "cat", "don't".
const AFF: &str = "SET UTF-8\nTRY esianrtolcdugmphbyfvkwz'\n\nSFX S Y 1\nSFX S 0 s .\n\nSFX D Y 1\nSFX D 0 ed .\n\nSFX G Y 1\nSFX G 0 ing .\n";
const DIC: &str = "4\nwalk/SDG\nthe\ncat/S\ndon't\n";

fn setup(
    cx: &mut TestAppContext,
) -> (Entity<Workspace>, &mut VisualTestContext, tempfile::TempDir) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().paths.user_settings = Some(dir.path().join("settings.toml"));
    });
    (workspace, cx, dir)
}

fn add_dictionary(dir: &tempfile::TempDir) {
    let folder = dir.path().join("dictionaries");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("xx_TEST.aff"), AFF).unwrap();
    std::fs::write(folder.join("xx_TEST.dic"), DIC).unwrap();
}

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

fn type_text(workspace: &Entity<Workspace>, text: &str, cx: &mut VisualTestContext) {
    run(
        workspace,
        Invocation::with_args("edit.insert-text", json!({ "text": text })),
        cx,
    )
    .unwrap();
}

fn use_test_dictionary(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) {
    run(
        workspace,
        Invocation::with_args("spell.language", json!({ "language": "xx_TEST" })),
        cx,
    )
    .unwrap();
}

/// The misspelled words of the active document.
fn underlined_words(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        let buffer = view.read(cx).buffer.clone();
        let text = buffer.read(cx).doc().text().clone();
        underlined(buffer.read(cx), buffer.entity_id(), 0..text.len(), cx)
            .iter()
            .map(|range| text.slice(range.clone()).to_string())
            .collect()
    })
}

fn selected(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    workspace.read_with(cx, |workspace, cx| {
        workspace
            .active_view(cx)
            .unwrap()
            .read(cx)
            .selected_text(cx)
            .unwrap_or_default()
    })
}

#[gpui_kit::test]
fn underlines_and_goes_through_misspelled_words(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    add_dictionary(&dir);
    use_test_dictionary(&workspace, cx);
    type_text(&workspace, "The cat wlaks, the dgo walked.", cx);
    // Nothing is underlined until it is turned on.
    assert!(underlined_words(&workspace, cx).is_empty());
    run(&workspace, Invocation::new("spell.auto"), cx).unwrap();
    assert!(cx.update(|_, cx| is_enabled(cx)));
    assert_eq!(underlined_words(&workspace, cx), ["wlaks", "dgo"]);

    run(&workspace, Invocation::new("cursor.document-start"), cx).unwrap();
    run(&workspace, Invocation::new("spell.next"), cx).unwrap();
    assert_eq!(selected(&workspace, cx), "wlaks");
    run(&workspace, Invocation::new("spell.next"), cx).unwrap();
    assert_eq!(selected(&workspace, cx), "dgo");
    run(&workspace, Invocation::new("spell.next"), cx).unwrap();
    assert_eq!(selected(&workspace, cx), "wlaks");
    run(&workspace, Invocation::new("spell.previous"), cx).unwrap();
    assert_eq!(selected(&workspace, cx), "dgo");

    run(&workspace, Invocation::new("spell.auto"), cx).unwrap();
    assert!(underlined_words(&workspace, cx).is_empty());
}

#[gpui_kit::test]
fn suggests_and_replaces_a_word(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    add_dictionary(&dir);
    use_test_dictionary(&workspace, cx);
    type_text(&workspace, "the wlak", cx);
    run(&workspace, Invocation::new("spell.suggestions"), cx).unwrap();
    let first = workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        let popup = view.read(cx).completion.clone().unwrap();
        (popup.start, popup.items[0].label.clone())
    });
    assert_eq!(first, (4, "walk".to_owned()));
    workspace.update(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.update(cx, |view, cx| view.accept_completion(cx));
    });
    assert_eq!(active_text(&workspace, cx), "the walk");

    // A word spelled right, no word, and a word without suggestions.
    let error = run(&workspace, Invocation::new("spell.suggestions"), cx).unwrap_err();
    assert!(error.to_string().contains("spelled right"), "{error}");
    type_text(&workspace, " ", cx);
    assert!(run(&workspace, Invocation::new("spell.suggestions"), cx).is_err());
    type_text(&workspace, "qqqqqqqqqqqq", cx);
    let error = run(&workspace, Invocation::new("spell.suggestions"), cx).unwrap_err();
    assert!(error.to_string().contains("no suggestions"), "{error}");
}

#[gpui_kit::test]
fn adds_words_to_the_users_dictionary(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    add_dictionary(&dir);
    use_test_dictionary(&workspace, cx);
    run(&workspace, Invocation::new("spell.auto"), cx).unwrap();
    type_text(&workspace, "the Birchpad", cx);
    assert_eq!(underlined_words(&workspace, cx), ["Birchpad"]);
    run(&workspace, Invocation::new("spell.add-word"), cx).unwrap();
    assert!(underlined_words(&workspace, cx).is_empty());
    let words = std::fs::read_to_string(dir.path().join("dictionaries/user.txt")).unwrap();
    assert_eq!(words, "Birchpad\n");
    // Read again with the dictionary.
    use_test_dictionary(&workspace, cx);
    assert!(underlined_words(&workspace, cx).is_empty());
}

#[gpui_kit::test]
fn checks_only_comments_and_strings_in_code(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    add_dictionary(&dir);
    use_test_dictionary(&workspace, cx);
    run(&workspace, Invocation::new("spell.auto"), cx).unwrap();
    run(
        &workspace,
        Invocation::with_args("language.set", json!({ "language": "rust" })),
        cx,
    )
    .unwrap();
    type_text(&workspace, "fn wlak() { let s = \"dgo\"; } // tac\n", cx);
    cx.run_until_parked();
    assert_eq!(underlined_words(&workspace, cx), ["dgo", "tac"]);
}

#[gpui_kit::test]
fn says_when_there_is_no_dictionary(cx: &mut TestAppContext) {
    let (workspace, cx, _dir) = setup(cx);
    let error = run(
        &workspace,
        Invocation::with_args("spell.language", json!({ "language": "zz_NONE" })),
        cx,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("no zz_NONE dictionary"),
        "{error}"
    );
    assert!(run(&workspace, Invocation::new("spell.auto"), cx).is_err());
    assert!(!cx.update(|_, cx| is_enabled(cx)));
    assert!(run(&workspace, Invocation::new("spell.next"), cx).is_err());
    assert!(run(&workspace, Invocation::new("spell.add-word"), cx).is_err());
    for bad in ["", "../en_US", "a.b"] {
        let invocation = Invocation::with_args("spell.language", json!({ "language": bad }));
        assert!(run(&workspace, invocation, cx).is_err(), "{bad}");
    }
    assert_eq!(cx.update(|_, cx| language(cx)), "zz_NONE");
}

#[gpui_kit::test]
fn lists_languages_and_marks_the_one_in_use(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    add_dictionary(&dir);
    assert!(
        cx.update(|_, cx| languages(cx))
            .contains(&"xx_TEST".to_owned())
    );
    use_test_dictionary(&workspace, cx);
    // Nothing misspelled: nowhere to go.
    type_text(&workspace, "the cat", cx);
    assert!(run(&workspace, Invocation::new("spell.next"), cx).is_err());
    let on = Invocation::with_args("spell.language", json!({ "language": "xx_TEST" }));
    assert_eq!(is_checked(&on, false, "xx_TEST"), Some(true));
    assert_eq!(is_checked(&on, false, "en_US"), Some(false));
    assert_eq!(
        is_checked(&Invocation::new("spell.auto"), true, ""),
        Some(true)
    );
    assert_eq!(is_checked(&Invocation::new("file.new"), true, ""), None);
}

fn set_spell(cx: &mut VisualTestContext, change: impl FnOnce(&mut birchpad_config::SpellSettings)) {
    cx.update(|_, cx| change(&mut cx.global_mut::<AppState>().settings.spell));
}

#[gpui_kit::test]
fn checks_several_languages_together(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    add_dictionary(&dir);
    let folder = dir.path().join("dictionaries");
    std::fs::write(folder.join("yy_TEST.aff"), "SET UTF-8\n").unwrap();
    std::fs::write(folder.join("yy_TEST.dic"), "2\nhund\nkatze\n").unwrap();
    use_test_dictionary(&workspace, cx);
    run(&workspace, Invocation::new("spell.auto"), cx).unwrap();
    type_text(&workspace, "the hund walked to the vogel", cx);
    assert_eq!(underlined_words(&workspace, cx), ["hund", "vogel"]);
    set_spell(cx, |spell| spell.languages = "xx_TEST, yy_TEST".into());
    assert_eq!(
        cx.update(|_, cx| languages_checked(cx)),
        ["xx_TEST", "yy_TEST"]
    );
    run(&workspace, Invocation::new("spell.auto"), cx).unwrap();
    run(&workspace, Invocation::new("spell.auto"), cx).unwrap();
    assert_eq!(underlined_words(&workspace, cx), ["vogel"]);
    // A language without a dictionary: nothing underlined, and the reason when asked.
    set_spell(cx, |spell| spell.languages = "xx_TEST zz_NONE".into());
    assert!(underlined_words(&workspace, cx).is_empty());
    let error = run(&workspace, Invocation::new("spell.next"), cx).unwrap_err();
    assert!(error.to_string().contains("zz_NONE"), "{error}");
}

#[gpui_kit::test]
fn the_rules_leave_words_alone(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    add_dictionary(&dir);
    use_test_dictionary(&workspace, cx);
    run(&workspace, Invocation::new("spell.auto"), cx).unwrap();
    type_text(&workspace, "the Wlak NASAA walk2", cx);
    assert_eq!(underlined_words(&workspace, cx), ["Wlak"]);
    set_spell(cx, |spell| {
        spell.ignore_capital_start = true;
        spell.ignore_all_caps = false;
        spell.ignore_numbers = false;
    });
    assert_eq!(underlined_words(&workspace, cx), ["NASAA", "walk2"]);
}

#[gpui_kit::test]
fn checks_code_by_style(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    add_dictionary(&dir);
    use_test_dictionary(&workspace, cx);
    run(&workspace, Invocation::new("spell.auto"), cx).unwrap();
    run(
        &workspace,
        Invocation::with_args("language.set", json!({ "language": "rust" })),
        cx,
    )
    .unwrap();
    type_text(&workspace, "// the wlak\nlet dgo = \"the cta\";\n", cx);
    // The parser runs in the background.
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    let found = |cx: &mut VisualTestContext| underlined_words(&workspace, cx);
    assert_eq!(found(cx), ["wlak", "cta"]);
    for (scope, expected) in [
        (birchpad_config::SpellScope::Comments, vec!["wlak"]),
        (birchpad_config::SpellScope::Strings, vec!["cta"]),
        (
            birchpad_config::SpellScope::Everything,
            vec!["wlak", "dgo", "cta"],
        ),
    ] {
        set_spell(cx, |spell| spell.check = scope);
        let mut words = found(cx);
        words.retain(|word| word != "let");
        assert_eq!(words, expected, "{scope:?}");
    }
}

#[gpui_kit::test]
fn us_english_is_built_in(cx: &mut TestAppContext) {
    let (workspace, cx, _dir) = setup(cx);
    // No dictionaries folder, no system dictionary needed.
    assert!(
        cx.update(|_, cx| languages(cx))
            .contains(&BUILT_IN_LANGUAGE.to_owned())
    );
    run(
        &workspace,
        Invocation::with_args("spell.language", json!({ "language": "en_US" })),
        cx,
    )
    .unwrap();
    run(&workspace, Invocation::new("spell.auto"), cx).unwrap();
    type_text(
        &workspace,
        "The quick brwn fox jumps over the lazy dog's kennel.",
        cx,
    );
    assert_eq!(underlined_words(&workspace, cx), ["brwn"]);
}
