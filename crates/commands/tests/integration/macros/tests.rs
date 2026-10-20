#[test]
fn searches_are_recorded_their_own_way() {
    for command in [
        "search.find-next",
        "search.find-previous",
        "search.find-text",
        "search.replace-text",
        "macro.trim-and-save",
        "file.exit",
    ] {
        assert!(!birchpad_commands::is_recordable(command), "{command}");
    }
    assert!(birchpad_commands::is_recordable(
        "search.select-and-find-next"
    ));
    assert!(birchpad_commands::is_recordable("edit.trim"));
}

use birchpad_commands::*;
use serde_json::json;

fn typed(text: &str) -> Invocation {
    Invocation::with_args("edit.type", json!({ "text": text }))
}

#[test]
fn typing_joins_into_one_step() {
    let mut steps = Vec::new();
    record(&mut steps, typed("a"));
    record(&mut steps, typed("bc"));
    record(&mut steps, Invocation::new("edit.newline"));
    record(&mut steps, typed("d"));
    assert_eq!(
        steps,
        [typed("abc"), Invocation::new("edit.newline"), typed("d")]
    );
    // A first step, and typed text without text, stay as they are.
    let mut steps = Vec::new();
    record(&mut steps, Invocation::new("edit.type"));
    record(&mut steps, typed("x"));
    assert_eq!(steps, [Invocation::new("edit.type"), typed("x")]);
}

#[test]
fn what_is_recorded() {
    for command in ["edit.type", "file.save", "search.mark", "edit.undo"] {
        assert!(is_recordable(command), "{command}");
    }
    for command in [
        "macro.playback",
        "macro.start-recording",
        "settings.preferences",
        "help.about",
        "file.exit",
    ] {
        assert!(!is_recordable(command), "{command}");
    }
}

#[test]
fn macros_round_trip() {
    let macros = vec![
        Macro {
            name: "Quote".into(),
            steps: vec![typed("\"x\\n\""), Invocation::new("edit.newline")],
        },
        Macro {
            name: "Empty".into(),
            steps: vec![],
        },
        Macro {
            name: "Case".into(),
            steps: vec![Invocation::with_args(
                "edit.convert-case",
                json!({ "to": "upper", "n": 3, "flag": true, "list": [1, 2] }),
            )],
        },
    ];
    let text = write_macros(&macros);
    assert_eq!(parse_macros(&text).unwrap(), macros, "{text}");
    assert_eq!(parse_macros("").unwrap(), vec![]);
    assert_eq!(parse_macros(&write_macros(&[])).unwrap(), vec![]);
}

#[test]
fn refuses_bad_files() {
    for text in [
        "[[macro]",
        "macro = 3",
        "[[macro]]\nsteps = []\n",
        "[[macro]]\nname = ' '\nsteps = []\n",
        "[[macro]]\nname = 'a'\n",
        "[[macro]]\nname = 'a'\nsteps = [{ args = {} }]\n",
        "[[macro]]\nname = 'a'\nsteps = [{ command = 'no.such' }]\n",
        "[[macro]]\nname = 'a'\nsteps = []\n[[macro]]\nname = 'a'\nsteps = []\n",
    ] {
        assert!(parse_macros(text).is_err(), "{text}");
    }
}
