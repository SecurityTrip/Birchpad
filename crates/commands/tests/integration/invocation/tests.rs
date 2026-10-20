use birchpad_commands::*;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct Eol {
    eol: String,
}

#[derive(Debug, Deserialize, PartialEq, Default)]
#[serde(default)]
struct Optional {
    count: u32,
}

#[test]
fn round_trips_through_json() {
    let invocation = Invocation::with_args("edit.convert-eol", Eol { eol: "lf".into() });
    let json = serde_json::to_string(&invocation).unwrap();
    assert_eq!(
        json,
        r#"{"command":"edit.convert-eol","args":{"eol":"lf"}}"#
    );
    let back: Invocation = serde_json::from_str(&json).unwrap();
    assert_eq!(back, invocation);
    assert_eq!(back.args::<Eol>().unwrap().eol, "lf");

    let bare: Invocation = serde_json::from_str(r#"{"command":"file.new"}"#).unwrap();
    assert_eq!(bare, Invocation::new("file.new"));
}

#[test]
fn missing_args_use_defaults_but_wrong_args_fail() {
    assert!(Invocation::new("file.new").args::<()>().is_ok());
    assert_eq!(
        Invocation::new("x.y").args::<Optional>().unwrap(),
        Optional::default()
    );
    let error = Invocation::new("edit.convert-eol")
        .args::<Eol>()
        .unwrap_err();
    assert_eq!(error.command, "edit.convert-eol");
    let wrong = Invocation::with_args("edit.convert-eol", serde_json::json!({ "eol": 1 }));
    assert!(wrong.args::<Eol>().is_err());
}
