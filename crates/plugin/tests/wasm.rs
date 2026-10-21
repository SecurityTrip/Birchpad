//! The plugin runtime with a real component: `fixtures/probe.wasm`, built from
//! `fixtures/probe` (see its `build.md`).

use std::sync::OnceLock;

use birchpad_commands::Invocation;
use birchpad_core::{Range, Rope, Selection};
use birchpad_plugin::wasm::Runtime;
use birchpad_plugin::{
    Control, DocumentState, Effect, EventKind, Grants, Outcome, Plugin, Session,
};
use wasmtime::component::Component;

const PROBE: &[u8] = include_bytes!("fixtures/probe.wasm");

fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    // Little fuel, so that the plugin that spins stops soon.
    RUNTIME.get_or_init(|| Runtime::with_fuel(200_000_000).unwrap())
}

fn compiled() -> Component {
    static COMPONENT: OnceLock<Component> = OnceLock::new();
    COMPONENT
        .get_or_init(|| runtime().compile(PROBE).unwrap())
        .clone()
}

fn probe() -> Plugin {
    runtime().load(compiled()).unwrap()
}

fn session(text: &str, ranges: &[(usize, usize)]) -> Session {
    Session::new(DocumentState {
        text: Rope::from(text),
        selection: Selection::new(ranges.iter().map(|&(a, h)| Range::new(a, h)), 0),
        path: Some("notes.txt".into()),
        language: "text".into(),
    })
}

fn applied(outcome: &Outcome, text: &str) -> String {
    let mut rope = Rope::from(text);
    if let Some(transaction) = &outcome.transaction {
        transaction.changes().apply(&mut rope);
    }
    rope.to_string()
}

#[test]
fn lists_its_commands() {
    let plugin = probe();
    let ids: Vec<&str> = plugin.commands().iter().map(|c| c.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "upper",
            "hello",
            "select-all",
            "describe",
            "panel",
            "fail",
            "bad-range",
            "spin",
            "trap",
            "grow",
            "settings",
            "files",
            "controls",
            "tasks"
        ]
    );
    assert_eq!(plugin.commands()[0].title, "Upper Case");
    // Only Upper Case goes into the text's context menu.
    let in_menu: Vec<&str> = plugin
        .commands()
        .iter()
        .filter(|c| c.context_menu)
        .map(|c| c.id.as_str())
        .collect();
    assert_eq!(in_menu, ["upper"]);
}

fn grants(settings: &[(&str, &str)], files: bool) -> Grants {
    Grants {
        settings: settings
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect(),
        files,
    }
}

#[test]
fn reads_and_keeps_its_settings() {
    let mut plugin = probe();
    let outcome = plugin
        .run(
            "settings",
            Session::empty().with_grants(grants(&[("greeting", "hi"), ("runs", "41")], false)),
        )
        .unwrap();
    assert_eq!(
        outcome.effects,
        [
            Effect::SetSetting {
                key: "runs".into(),
                value: "42".into()
            },
            Effect::Message("greeting hi, runs 42".into()),
        ]
    );
    // None set yet: the plugin starts from nothing.
    let outcome = plugin.run("settings", Session::empty()).unwrap();
    assert_eq!(
        outcome.effects[1],
        Effect::Message("greeting (none), runs 1".into())
    );
}

#[test]
fn uses_files_only_when_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.txt");
    let target = dir.path().join("target.txt");
    std::fs::write(&source, "copy me").unwrap();
    let paths = [
        ("source", source.to_str().unwrap()),
        ("target", target.to_str().unwrap()),
    ];
    let mut plugin = probe();

    // Not allowed: refused, and nothing written.
    let error = plugin
        .run("files", Session::empty().with_grants(grants(&paths, false)))
        .unwrap_err();
    assert!(error.to_string().contains("may not use files"), "{error}");
    assert!(!target.exists());

    let outcome = plugin
        .run("files", Session::empty().with_grants(grants(&paths, true)))
        .unwrap();
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "COPY ME");
    assert_eq!(outcome.effects, [Effect::OpenFile(target.clone())]);

    // A file that is not there: the plugin's error says which.
    std::fs::remove_file(&source).unwrap();
    let error = plugin
        .run("files", Session::empty().with_grants(grants(&paths, true)))
        .unwrap_err();
    assert!(error.to_string().contains("source.txt"), "{error}");
}

#[test]
fn shows_controls_and_hears_them() {
    let mut plugin = probe();
    let outcome = plugin.run("controls", Session::empty()).unwrap();
    assert_eq!(
        outcome.effects,
        [Effect::Controls {
            title: "Probe controls".into(),
            controls: vec![
                Control::Label("A label".into()),
                Control::Button {
                    id: "go".into(),
                    title: "Go".into()
                },
                Control::Separator,
                Control::TextField {
                    id: "name".into(),
                    value: "typed".into()
                },
                Control::Checkbox {
                    id: "loud".into(),
                    title: "Loud".into(),
                    checked: true
                },
            ]
        }]
    );
    let outcome = plugin.control("loud", "false", Session::empty()).unwrap();
    assert_eq!(
        outcome.effects,
        [Effect::Message("control loud = false".into())]
    );
    // An empty value, as a button gives.
    let outcome = plugin.control("go", "", Session::empty()).unwrap();
    assert_eq!(outcome.effects, [Effect::Message("control go = ".into())]);
}

#[test]
fn hears_the_command_line_message() {
    let mut plugin = probe();
    for message in ["open the log", "", "мир"] {
        let outcome = plugin.message(message, Session::empty()).unwrap();
        assert_eq!(
            outcome.effects,
            [Effect::Message(format!("message {message}"))]
        );
    }
    // The message is no event of on-event.
    let error = plugin
        .event(EventKind::Message, Session::empty())
        .unwrap_err();
    assert!(error.to_string().contains("on-message"), "{error}");
}

#[test]
fn starts_tasks_that_run_apart() {
    let mut plugin = probe();
    let outcome = plugin.run("tasks", session("sum fail", &[(0, 0)])).unwrap();
    assert_eq!(
        outcome.effects,
        [
            Effect::StartTask {
                task: "sum".into(),
                input: "2 3 4".into()
            },
            Effect::StartTask {
                task: "fail".into(),
                input: "2 3 4".into()
            },
        ]
    );
    let runner = plugin.task_runner();
    // On another thread, as Birchpad runs it, with no document.
    let (returned, outcome) =
        std::thread::spawn(move || runner.run("sum", "2 3 4", Session::empty()))
            .join()
            .unwrap()
            .unwrap();
    assert_eq!(returned, Ok("9".to_owned()));
    assert_eq!(
        outcome.effects,
        [Effect::Message("summing, document of 0 bytes".into())]
    );
    let runner = plugin.task_runner();
    let (returned, _) = runner.run("fail", "", Session::empty()).unwrap();
    assert_eq!(returned, Err("the task failed on purpose".to_owned()));

    // What it returned goes back to the plugin.
    let outcome = plugin
        .task_done("sum", &Ok("9".to_owned()), Session::empty())
        .unwrap();
    assert_eq!(outcome.effects, [Effect::Message("task sum gave 9".into())]);
    let outcome = plugin
        .task_done("fail", &Err("no".to_owned()), Session::empty())
        .unwrap();
    assert_eq!(
        outcome.effects,
        [Effect::Message("task fail failed: no".into())]
    );
}

#[test]
fn stops_a_task_that_runs_too_long() {
    let plugin = probe();
    let error = plugin
        .task_runner()
        .run("spin", "", Session::empty())
        .err()
        .unwrap();
    assert_eq!(error.to_string(), "the plugin ran too long and was stopped");
}

#[test]
fn starts_no_more_tasks_than_a_call_may() {
    let mut plugin = probe();
    let names = vec!["sum"; birchpad_plugin::session::MAX_TASKS + 1].join(" ");
    let error = plugin.run("tasks", session(&names, &[(0, 0)])).unwrap_err();
    assert!(error.to_string().contains("at most"), "{error}");
    let names = vec!["sum"; birchpad_plugin::session::MAX_TASKS].join(" ");
    let outcome = plugin.run("tasks", session(&names, &[(0, 0)])).unwrap();
    assert_eq!(outcome.effects.len(), birchpad_plugin::session::MAX_TASKS);
}

#[test]
fn edits_every_selection_in_one_transaction() {
    let mut plugin = probe();
    let text = "one two three";
    let outcome = plugin
        .run("upper", session(text, &[(0, 3), (8, 13)]))
        .unwrap();
    assert_eq!(applied(&outcome, text), "ONE two THREE");
    assert!(outcome.effects.is_empty());

    let outcome = plugin.run("hello", session("ab", &[(1, 1)])).unwrap();
    assert_eq!(applied(&outcome, "ab"), "aHello from probeb");
    assert_eq!(outcome.effects, [Effect::Message("said hello".into())]);
}

#[test]
fn reads_the_document_and_sets_the_selection() {
    let mut plugin = probe();
    let outcome = plugin.run("describe", session("мир", &[(0, 0)])).unwrap();
    assert_eq!(
        outcome.effects,
        [Effect::Message("6 bytes, text, notes.txt, \"мир\"".into())]
    );
    let transaction = outcome.transaction.unwrap();
    assert!(transaction.changes().is_identity());
    assert_eq!(transaction.selection().unwrap().primary(), Range::new(0, 6));

    // Without a document: nothing to read, and the selection is refused.
    let error = plugin.run("describe", Session::empty()).unwrap_err();
    assert!(error.to_string().contains("no document"), "{error}");
}

#[test]
fn asks_for_commands_and_panels() {
    let mut plugin = probe();
    let outcome = plugin.run("select-all", session("", &[(0, 0)])).unwrap();
    assert_eq!(
        outcome.effects,
        [Effect::RunCommand(Invocation::new("edit.select-all"))]
    );
    let outcome = plugin.run("panel", session("", &[(0, 0)])).unwrap();
    assert_eq!(
        outcome.effects,
        [Effect::Panel {
            title: "Probe".into(),
            lines: vec!["one".into(), "two".into()]
        }]
    );
    let outcome = plugin.panel_click(1, Session::empty()).unwrap();
    assert_eq!(outcome.effects, [Effect::Message("clicked 1".into())]);
}

#[test]
fn hears_events() {
    let mut plugin = probe();
    for (event, name) in [
        (EventKind::Opened, "Opened"),
        (EventKind::Saved, "Saved"),
        (EventKind::Closed, "Closed"),
        (EventKind::Activated, "Activated"),
        (EventKind::Changed, "Changed"),
        (EventKind::SelectionChanged, "SelectionChanged"),
    ] {
        let outcome = plugin.event(event, Session::empty()).unwrap();
        assert_eq!(
            outcome.effects,
            [Effect::Message(format!("event Event::{name}"))]
        );
    }
}

#[test]
fn reports_failures_and_keeps_working() {
    let mut plugin = probe();
    let error = plugin.run("fail", session("x", &[(0, 0)])).unwrap_err();
    assert_eq!(error.to_string(), "it failed on purpose");
    let error = plugin
        .run("bad-range", session("x", &[(0, 0)]))
        .unwrap_err();
    assert!(error.to_string().contains("past the text"), "{error}");
    let error = plugin.run("no-such", session("x", &[(0, 0)])).unwrap_err();
    assert!(error.to_string().contains("no command no-such"), "{error}");

    let error = plugin.run("trap", session("x", &[(0, 0)])).unwrap_err();
    assert!(error.to_string().contains("the plugin failed"), "{error}");
    // A fresh instance takes the next call.
    let outcome = plugin.run("upper", session("x", &[(0, 1)])).unwrap();
    assert_eq!(applied(&outcome, "x"), "X");
}

#[test]
fn stops_a_plugin_that_runs_too_long() {
    let mut plugin = probe();
    let error = plugin.run("spin", session("x", &[(0, 0)])).unwrap_err();
    assert_eq!(error.to_string(), "the plugin ran too long and was stopped");
    assert!(plugin.run("hello", session("", &[(0, 0)])).is_ok());
}

#[test]
fn stops_a_plugin_that_takes_too_much_memory() {
    let mut plugin = probe();
    // It stops at the memory limit, or runs out of fuel filling memory on the way there.
    let error = plugin.run("grow", session("x", &[(0, 0)])).unwrap_err();
    assert!(error.to_string().starts_with("the plugin"), "{error}");
    assert!(plugin.run("hello", session("", &[(0, 0)])).is_ok());
}

#[test]
fn refuses_what_is_not_a_plugin() {
    let runtime = runtime();
    assert!(runtime.compile(b"").is_err());
    assert!(runtime.compile(b"not wasm at all").is_err());
    // A component that exports nothing does not start.
    let empty = runtime.compile(b"\0asm\x0d\0\x01\0").unwrap();
    let error = runtime.load(empty).err().unwrap();
    assert!(error.to_string().contains("does not start"), "{error}");
}
