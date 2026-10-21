use birchpad_plugin::manifest::*;

const GOOD: &str = r#"
id = "word-count"
name = "Word Count"
version = "1.0.0"
api = "0.1"
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
        (("api = \"0.1\"", "api = \"9.0\""), "plugin API 9.0"),
        (
            ("api = \"0.1\"", "api = \"0.1\"\nentry = \"../x.wasm\""),
            "entry",
        ),
        (
            ("api = \"0.1\"", "api = \"0.1\"\nentry = \"x.dll\""),
            "entry",
        ),
        (
            ("api = \"0.1\"", "api = \"0.1\"\nentry = \".wasm\""),
            "entry",
        ),
        (
            ("api = \"0.1\"", "api = \"0.1\"\nentry = \"C:x.wasm\""),
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
