use birchpad_commands::Invocation;
use birchpad_core::{Range, Rope, Selection};
use serde_json::json;

use birchpad_plugin::script::*;
use birchpad_plugin::session::Outcome;
use birchpad_plugin::session::Session;
use birchpad_plugin::session::{DocumentState, Effect};

fn session(text: &str, anchor: usize, head: usize) -> Session {
    Session::new(DocumentState {
        text: Rope::from(text),
        selection: Selection::single(Range::new(anchor, head)),
        path: None,
        language: "text".into(),
    })
}

fn edited(source: &str, text: &str, anchor: usize, head: usize) -> (String, Outcome) {
    let outcome = run("test", source, session(text, anchor, head)).unwrap();
    let mut rope = Rope::from(text);
    if let Some(transaction) = &outcome.transaction {
        transaction.changes().apply(&mut rope);
    }
    (rope.to_string(), outcome)
}

#[test]
fn edits_the_document() {
    let (text, _) = edited(r#"set_text(text().to_upper());"#, "hello", 0, 0);
    assert_eq!(text, "HELLO");
    let (text, outcome) = edited(r#"insert("[" + selected_text() + "]");"#, "a b c", 2, 3);
    assert_eq!(text, "a [b] c");
    assert_eq!(
        outcome.transaction.unwrap().selection().unwrap().primary(),
        Range::point(5)
    );
    let (text, _) = edited(
        r#"replace(0, 1, "X"); let t = text_range(1, length()); replace(1, length(), t + "!");"#,
        "abc",
        0,
        0,
    );
    assert_eq!(text, "Xbc!");
}

#[test]
fn reads_selections_and_the_document() {
    let (_, outcome) = edited(
        r#"let s = selections(); message(`${s[0].anchor}-${s[0].head} ${language()} ${path() == ()}`);
               set_selection(0, 1); print("printed");"#,
        "abc",
        3,
        1,
    );
    assert_eq!(
        outcome.effects,
        [
            Effect::Message("3-1 text true".into()),
            Effect::Message("printed".into())
        ]
    );
    assert_eq!(
        outcome.transaction.unwrap().selection().unwrap().primary(),
        Range::new(0, 1)
    );
}

#[test]
fn asks_for_commands_and_panels() {
    let (_, outcome) = edited(
        r#"run_command("edit.select-all"); run_command("edit.insert-text", #{ text: "x" });
               show_panel("Lines", ["a", 1]);"#,
        "",
        0,
        0,
    );
    assert_eq!(
        outcome.effects,
        [
            Effect::RunCommand(Invocation::new("edit.select-all")),
            Effect::RunCommand(Invocation::with_args(
                "edit.insert-text",
                json!({ "text": "x" })
            )),
            Effect::Panel {
                title: "Lines".into(),
                lines: vec!["a".into(), "1".into()]
            },
        ]
    );
}

#[test]
fn reports_errors_and_applies_nothing() {
    for (source, expected) in [
        ("replace(5, 9, \"x\");", "past the text"),
        ("replace(-1, 0, \"x\");", "not a position"),
        ("set_selection(0, 99);", "past the text"),
        ("run_command(\"no.such\");", "no command"),
        ("let x = ;", "the script test failed"),
        ("throw \"mine\";", "mine"),
        ("loop {}", "ran too long"),
        ("no_such_function();", "no_such_function"),
    ] {
        let error = run("test", source, session("abc", 0, 0)).unwrap_err();
        assert!(error.to_string().contains(expected), "{source}: {error}");
    }
}

#[test]
fn an_empty_script_does_nothing() {
    assert_eq!(
        run("empty", "", session("abc", 0, 0)).unwrap(),
        Outcome::default()
    );
    let outcome = run("none", "insert(\"x\");", Session::empty());
    assert!(outcome.is_err());
}

#[test]
fn checks_scripts_without_running_them() {
    assert!(check("let x = 1; message(\"never\");").is_ok());
    assert!(check("let x = ;").is_err());
    assert!(check("").is_ok());
}
