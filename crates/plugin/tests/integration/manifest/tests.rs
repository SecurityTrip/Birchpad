use birchpad_plugin::manifest::*;

const GOOD: &str = r#"
id = "word-count"
name = "Word Count"
version = "1.0.0"
api = "0.2"
events = ["saved", "opened"]
"#;

#[test]
fn reads_a_manifest_with_defaults() {
    let manifest = Manifest::parse(GOOD).unwrap();
    assert_eq!(manifest.id, "word-count");
    assert_eq!(manifest.entry, "plugin.wasm");
    assert_eq!(manifest.description, "");
    assert_eq!(manifest.events, [EventKind::Saved, EventKind::Opened]);
    assert_eq!(Manifest::parse(&manifest.to_toml()).unwrap(), manifest);
}

#[test]
fn checks_ids() {
    for id in ["a", "word-count", "x2", &"a".repeat(MAX_ID)] {
        assert!(is_valid_id(id), "{id}");
    }
    for id in [
        "",
        "-a",
        "a-",
        "A",
        "a_b",
        "a.b",
        "a b",
        "../x",
        &"a".repeat(MAX_ID + 1),
    ] {
        assert!(!is_valid_id(id), "{id}");
    }
}

#[test]
fn refuses_what_does_not_make_sense() {
    for (change, expected) in [
        (("id = \"word-count\"", "id = \"Word\""), "not a plugin id"),
        (("name = \"Word Count\"", "name = \"  \""), "needs a name"),
        (
            ("name = \"Word Count\"", "name = \"a\\nb\""),
            "needs a name",
        ),
        (("version = \"1.0.0\"", "version = \"\""), "needs a version"),
        (("api = \"0.2\"", "api = \"9.0\""), "plugin API 9.0"),
        (
            ("api = \"0.2\"", "api = \"0.2\"\nentry = \"../x.wasm\""),
            "entry",
        ),
        (
            ("api = \"0.2\"", "api = \"0.2\"\nentry = \"x.dll\""),
            "entry",
        ),
        (
            ("api = \"0.2\"", "api = \"0.2\"\nentry = \".wasm\""),
            "entry",
        ),
        (
            ("api = \"0.2\"", "api = \"0.2\"\nentry = \"C:x.wasm\""),
            "entry",
        ),
    ] {
        let text = GOOD.replace(change.0, change.1);
        let error = Manifest::parse(&text).unwrap_err().to_string();
        assert!(error.contains(expected), "{text}: {error}");
    }
    let long = GOOD.replace("Word Count", &"n".repeat(MAX_NAME + 1));
    assert!(Manifest::parse(&long).is_err());
    let longest = GOOD.replace("Word Count", &"n".repeat(MAX_NAME));
    assert!(Manifest::parse(&longest).is_ok());
}

#[test]
fn refuses_what_does_not_read() {
    for text in [
        "",
        "id = ",
        "id = \"a\"",
        &format!("{GOOD}\nunknown = 1"),
        &GOOD.replace("\"saved\"", "\"typed\""),
    ] {
        assert!(
            matches!(Manifest::parse(text), Err(ManifestError::Syntax(_))),
            "{text}"
        );
    }
}

const WITH_SETTINGS: &str = r#"
id = "word-count"
name = "Word Count"
version = "1.0.0"
api = "0.2"
events = ["changed", "selection-changed", "message"]
permissions = ["files"]

[[settings]]
key = "count-numbers"
title = "Count numbers as words"
kind = "bool"
default = "true"

[[settings]]
key = "limit"
title = "Limit"
kind = "number"

[[settings]]
key = "greeting"
title = "Greeting"
kind = "text"
default = "Hello"
"#;

#[test]
fn reads_permissions_settings_and_the_new_events() {
    let manifest = Manifest::parse(WITH_SETTINGS).unwrap();
    assert_eq!(
        manifest.events,
        [
            EventKind::Changed,
            EventKind::SelectionChanged,
            EventKind::Message
        ]
    );
    assert!(manifest.asks_for(Permission::Files));
    assert_eq!(manifest.settings.len(), 3);
    assert_eq!(manifest.setting("limit").unwrap().kind, SettingKind::Number);
    assert_eq!(manifest.setting("limit").unwrap().default, "");
    assert_eq!(manifest.setting("greeting").unwrap().default, "Hello");
    assert!(manifest.setting("missing").is_none());
    assert_eq!(Manifest::parse(&manifest.to_toml()).unwrap(), manifest);
    // Without them: no permission, no settings.
    let plain = Manifest::parse(GOOD).unwrap();
    assert!(!plain.asks_for(Permission::Files));
    assert!(plain.settings.is_empty());
    assert!(!plain.to_toml().contains("permissions"));
}

#[test]
fn settings_take_the_values_of_their_kind() {
    let manifest = Manifest::parse(WITH_SETTINGS).unwrap();
    let flag = manifest.setting("count-numbers").unwrap();
    assert!(flag.accepts("true") && flag.accepts("false"));
    for value in ["", "yes", "TRUE", "1"] {
        assert!(!flag.accepts(value), "{value:?}");
    }
    let limit = manifest.setting("limit").unwrap();
    for value in ["0", "-1", &i64::MAX.to_string(), &i64::MIN.to_string()] {
        assert!(limit.accepts(value), "{value:?}");
    }
    for value in ["", "1.5", "ten", &format!("{}0", i64::MAX)] {
        assert!(!limit.accepts(value), "{value:?}");
    }
    let greeting = manifest.setting("greeting").unwrap();
    assert!(greeting.accepts("") && greeting.accepts("мир"));
    assert!(!greeting.accepts("two\nlines") && !greeting.accepts("a\rb"));
}

#[test]
fn refuses_settings_that_do_not_make_sense() {
    for (change, expected) in [
        (("key = \"limit\"", "key = \"Limit\""), "not a setting key"),
        (("key = \"limit\"", "key = \"greeting\""), "comes twice"),
        (("title = \"Limit\"", "title = \" \""), "title of one line"),
        (
            ("title = \"Limit\"", "title = \"a\\nb\""),
            "title of one line",
        ),
        (
            ("default = \"true\"", "default = \"yes\""),
            "not a value it takes",
        ),
        (
            ("default = \"Hello\"", "default = \"a\\nb\""),
            "not a value it takes",
        ),
    ] {
        let text = WITH_SETTINGS.replace(change.0, change.1);
        let error = Manifest::parse(&text).unwrap_err().to_string();
        assert!(error.contains(expected), "{text}: {error}");
    }
    for text in [
        WITH_SETTINGS.replace("\"files\"", "\"network\""),
        WITH_SETTINGS.replace("kind = \"number\"", "kind = \"date\""),
        WITH_SETTINGS.replace("kind = \"number\"", ""),
    ] {
        assert!(
            matches!(Manifest::parse(&text), Err(ManifestError::Syntax(_))),
            "{text}"
        );
    }
}

#[test]
fn declares_no_more_settings_than_the_limit() {
    let setting = |index: usize| {
        format!("\n[[settings]]\nkey = \"s{index}\"\ntitle = \"S\"\nkind = \"text\"\n")
    };
    let most: String = (0..MAX_DECLARED_SETTINGS).map(setting).collect();
    assert!(Manifest::parse(&format!("{GOOD}{most}")).is_ok());
    let error = Manifest::parse(&format!("{GOOD}{most}{}", setting(MAX_DECLARED_SETTINGS)))
        .unwrap_err()
        .to_string();
    assert!(error.contains("settings, more than"), "{error}");
}
