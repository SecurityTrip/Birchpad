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

fn granted(files: bool) -> Session {
    Session::empty().with_grants(Grants {
        settings: [("greeting".to_owned(), "hi".to_owned())].into(),
        files,
    })
}

#[test]
fn reads_and_sets_settings() {
    let mut s = granted(false);
    assert_eq!(s.setting("greeting").as_deref(), Some("hi"));
    assert_eq!(s.setting("missing"), None);
    s.set_setting("greeting", "hello").unwrap();
    s.set_setting("count.max_2-b", "").unwrap();
    // The call sees what it set.
    assert_eq!(s.setting("greeting").as_deref(), Some("hello"));
    assert_eq!(s.setting("count.max_2-b").as_deref(), Some(""));
    assert_eq!(
        s.finish().effects,
        [
            Effect::SetSetting {
                key: "greeting".into(),
                value: "hello".into()
            },
            Effect::SetSetting {
                key: "count.max_2-b".into(),
                value: String::new()
            },
        ]
    );
}

#[test]
fn refuses_settings_that_are_not() {
    let mut s = granted(false);
    for key in [
        "",
        "Greeting",
        "a b",
        "a/b",
        "ключ",
        &"k".repeat(MAX_SETTING_KEY + 1),
    ] {
        assert!(s.set_setting(key, "x").is_err(), "{key:?}");
    }
    assert!(s.set_setting(&"k".repeat(MAX_SETTING_KEY), "x").is_ok());
    assert!(
        s.set_setting("big", &"v".repeat(MAX_SETTING_VALUE + 1))
            .is_err()
    );
    assert!(s.set_setting("big", &"v".repeat(MAX_SETTING_VALUE)).is_ok());
    assert!(is_setting_key("a.b_c-1"));
}

#[test]
fn keeps_no_more_settings_than_the_limit() {
    let mut s = Session::empty();
    for index in 0..MAX_SETTINGS {
        s.set_setting(&format!("k{index}"), "x").unwrap();
    }
    let error = s.set_setting("one-more", "x").unwrap_err();
    assert!(error.contains("at most"), "{error}");
    // One that is there already can still change.
    s.set_setting("k0", "y").unwrap();
}

#[test]
fn uses_files_only_when_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.txt");
    let name = path.to_str().unwrap();

    let mut refused = granted(false);
    for error in [
        refused.read_file(name).unwrap_err(),
        refused.write_file(name, "x").unwrap_err(),
        refused.open_file(name).unwrap_err(),
    ] {
        assert!(error.contains("may not use files"), "{error}");
    }
    assert!(!path.exists());

    let mut s = granted(true);
    s.write_file(name, "мир\n").unwrap();
    assert_eq!(s.read_file(name).unwrap(), "мир\n");
    s.write_file(name, "").unwrap();
    assert_eq!(s.read_file(name).unwrap(), "");
    s.open_file(name).unwrap();
    assert_eq!(s.finish().effects, [Effect::OpenFile(path.clone())]);
}

#[test]
fn refuses_files_it_cannot_use() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = granted(true);
    // Relative paths would depend on the folder Birchpad was started in.
    for path in ["notes.txt", "", "./x", "../x"] {
        assert!(
            s.read_file(path).unwrap_err().contains("absolute"),
            "{path:?}"
        );
        assert!(s.write_file(path, "x").is_err(), "{path:?}");
        assert!(s.open_file(path).is_err(), "{path:?}");
    }
    let missing = dir.path().join("missing.txt");
    let error = s.read_file(missing.to_str().unwrap()).unwrap_err();
    assert!(error.contains("missing.txt"), "{error}");
    let binary = dir.path().join("binary.bin");
    std::fs::write(&binary, [0xff, 0xfe, 0x00]).unwrap();
    let error = s.read_file(binary.to_str().unwrap()).unwrap_err();
    assert!(error.contains("UTF-8"), "{error}");
    // A folder is no file to write.
    assert!(s.write_file(dir.path().to_str().unwrap(), "x").is_err());
}

#[test]
fn checks_controls() {
    let mut s = Session::empty();
    let button = |id: &str| Control::Button {
        id: id.into(),
        title: "Go".into(),
    };
    s.show_controls("Empty", Vec::new()).unwrap();
    s.show_controls(
        "Two",
        vec![
            button("a"),
            Control::Label("text".into()),
            Control::Separator,
            Control::Separator,
        ],
    )
    .unwrap();
    let error = s
        .show_controls(
            "Twice",
            vec![
                button("a"),
                Control::TextField {
                    id: "a".into(),
                    value: String::new(),
                },
            ],
        )
        .unwrap_err();
    assert!(error.contains("comes twice"), "{error}");
    let most: Vec<Control> = (0..MAX_CONTROLS).map(|i| button(&i.to_string())).collect();
    s.show_controls("Most", most.clone()).unwrap();
    let error = s
        .show_controls("Too many", [most, vec![button("x")]].concat())
        .unwrap_err();
    assert!(error.contains("more than"), "{error}");
    assert_eq!(s.finish().effects.len(), 3);
    assert_eq!(button("b").id(), Some("b"));
    assert_eq!(Control::Separator.id(), None);
}

#[test]
fn starts_tasks_up_to_the_limit() {
    let mut s = Session::empty();
    for task in ["", "Sum", "a b", "-x"] {
        assert!(s.start_task(task, "").is_err(), "{task:?}");
    }
    for _ in 0..MAX_TASKS {
        s.start_task("sum", "1 2").unwrap();
    }
    let error = s.start_task("sum", "").unwrap_err();
    assert!(error.contains("at most"), "{error}");
    let effects = s.finish().effects;
    assert_eq!(effects.len(), MAX_TASKS);
    assert_eq!(
        effects[0],
        Effect::StartTask {
            task: "sum".into(),
            input: "1 2".into()
        }
    );
}

#[test]
fn has_a_document_or_not() {
    assert!(session("", 0, 0).has_document());
    assert!(!Session::empty().has_document());
}
