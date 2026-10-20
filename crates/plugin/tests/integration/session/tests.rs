use serde_json::json;

use birchpad_commands::Invocation;
use birchpad_core::Range;
use birchpad_core::Rope;
use birchpad_core::Selection;
use birchpad_plugin::session::*;

fn session(text: &str, anchor: usize, head: usize) -> Session {
    Session::new(DocumentState {
        text: Rope::from(text),
        selection: Selection::single(Range::new(anchor, head)),
        path: Some("a.txt".into()),
        language: "text".into(),
    })
}

fn result(session: Session, original: &str) -> (String, Option<Selection>) {
    let outcome = session.finish();
    let mut text = Rope::from(original);
    let Some(transaction) = outcome.transaction else {
        return (original.to_owned(), None);
    };
    transaction.changes().apply(&mut text);
    (text.to_string(), transaction.selection().cloned())
}

#[test]
fn edits_compose_into_one_transaction() {
    let mut s = session("hello world", 0, 5);
    assert_eq!(s.selected_text(), "hello");
    s.replace(0, 5, "HELLO").unwrap();
    s.replace(11, 11, "!").unwrap();
    assert_eq!(s.text(), "HELLO world!");
    assert_eq!(s.text_range(6, 11).unwrap(), "world");
    s.insert("Hi").unwrap();
    assert_eq!(s.text(), "Hi world!");
    assert_eq!(s.len(), 9);
    let (text, selection) = result(s, "hello world");
    assert_eq!(text, "Hi world!");
    assert_eq!(selection.unwrap().primary(), Range::point(2));
}

#[test]
fn sets_the_text_and_the_selections() {
    let mut s = session("abc", 0, 0);
    s.set_text("мир").unwrap();
    s.set_selections(&[(6, 2), (0, 0)]).unwrap();
    assert_eq!(s.selections(), [(6, 2), (0, 0)]);
    assert_eq!(s.selected_text(), "ир");
    let (text, selection) = result(s, "abc");
    assert_eq!(text, "мир");
    assert_eq!(selection.unwrap().ranges().len(), 2);
}

#[test]
fn nothing_done_is_no_transaction() {
    let s = session("abc", 1, 1);
    assert_eq!(s.selections(), [(1, 1)]);
    assert_eq!(s.path().as_deref(), Some("a.txt"));
    assert_eq!(s.language(), "text");
    assert_eq!(s.finish(), Outcome::default());
}

#[test]
fn refuses_bad_ranges_and_changes_nothing() {
    let mut s = session("мир", 0, 0);
    assert!(s.replace(2, 1, "").unwrap_err().contains("reversed"));
    assert!(s.replace(0, 7, "").unwrap_err().contains("past the text"));
    assert!(
        s.replace(1, 1, "")
            .unwrap_err()
            .contains("inside a character")
    );
    assert!(s.text_range(0, 1).is_err());
    assert!(s.set_selections(&[]).is_err());
    assert!(s.set_selections(&[(0, 99)]).is_err());
    assert_eq!(s.text_range(0, 0).unwrap(), "");
    assert_eq!(s.text_range(0, 6).unwrap(), "мир");
    assert_eq!(s.finish(), Outcome::default());
}

#[test]
fn without_a_document_reads_nothing_and_edits_nothing() {
    let mut s = Session::empty();
    assert_eq!(s.text(), "");
    assert!(s.is_empty());
    assert!(s.selections().is_empty());
    assert_eq!(s.selected_text(), "");
    assert_eq!(s.path(), None);
    assert_eq!(s.language(), "");
    assert!(s.replace(0, 0, "x").is_err());
    assert!(s.insert("x").is_err());
    assert!(s.set_text("x").is_err());
    assert!(s.set_selections(&[(0, 0)]).is_err());
    assert!(s.text_range(0, 0).is_err());
    s.message("still talks");
    let outcome = s.finish();
    assert_eq!(outcome.transaction, None);
    assert_eq!(outcome.effects, [Effect::Message("still talks".into())]);
}

#[test]
fn gathers_effects_in_order() {
    let mut s = Session::empty();
    s.run_command("edit.select-all", serde_json::Value::Null)
        .unwrap();
    s.run_command_json("edit.insert-text", r#"{"text":"x"}"#)
        .unwrap();
    s.run_command_json("edit.undo", "  ").unwrap();
    assert!(s.run_command("no.such", json!({})).is_err());
    assert!(s.run_command_json("edit.undo", "{oops").is_err());
    s.show_panel("Found", vec!["one".into()]);
    let effects = s.finish().effects;
    assert_eq!(
        effects,
        [
            Effect::RunCommand(Invocation::new("edit.select-all")),
            Effect::RunCommand(Invocation {
                command: "edit.insert-text".into(),
                args: json!({"text": "x"})
            }),
            Effect::RunCommand(Invocation::new("edit.undo")),
            Effect::Panel {
                title: "Found".into(),
                lines: vec!["one".into()]
            },
        ]
    );
}

#[test]
fn drops_effects_past_the_limit() {
    let mut s = Session::empty();
    for _ in 0..=MAX_EFFECTS {
        s.message("spam");
    }
    assert_eq!(s.finish().effects.len(), MAX_EFFECTS);
}
