use birchpad_config::*;
use toml::Table;
use toml::Value;

fn int(value: i64) -> Value {
    Value::Integer(value)
}

#[test]
fn sets_a_key_keeping_the_rest() {
    let text =
        "# My settings\n[editor]\n# four is too much\ntab-width = 4 # old\nword-wrap = true\n";
    let edited = edit_settings(text, "editor.tab-width", Some(&int(2))).unwrap();
    assert_eq!(
        edited,
        "# My settings\n[editor]\n# four is too much\ntab-width = 2 # old\nword-wrap = true\n"
    );
    // A new key goes into its table, which is made if needed.
    let edited = edit_settings(text, "editor.insert-spaces", Some(&Value::Boolean(true))).unwrap();
    assert!(edited.contains("insert-spaces = true"), "{edited}");
    let edited = edit_settings(
        "",
        "highlighting.smart.enabled",
        Some(&Value::Boolean(false)),
    )
    .unwrap();
    let table: Table = edited.parse().unwrap();
    assert_eq!(
        table["highlighting"]["smart"]["enabled"],
        Value::Boolean(false)
    );
    // Dotted keys and inline tables are tables too.
    let edited =
        edit_settings("editor.tab-width = 8\n", "editor.tab-width", Some(&int(3))).unwrap();
    assert_eq!(edited, "editor.tab-width = 3\n");
    let edited = edit_settings(
        "editor = { tab-width = 8 }\n",
        "editor.tab-width",
        Some(&int(3)),
    )
    .unwrap();
    assert_eq!(edited, "editor = { tab-width = 3 }\n");
}

#[test]
fn comments_alone_stay_on_top() {
    let edited = edit_settings("# mine\n", "toolbar.show", Some(&Value::Boolean(false)));
    assert_eq!(edited.unwrap(), "# mine\n[toolbar]\nshow = false\n");
    // Without a line end after them, and with blank lines.
    let edited = edit_settings("# a\n\n# b", "editor.tab-width", Some(&int(2))).unwrap();
    assert!(edited.starts_with("# a\n\n# b\n["), "{edited}");
    // Removing what is not there changes nothing.
    assert_eq!(
        edit_settings("# mine", "editor.tab-width", None).unwrap(),
        "# mine"
    );
    // A setting that does not exist is refused all the same.
    assert!(edit_settings("# mine\n", "no.such", Some(&int(1))).is_err());
}

#[test]
fn strings_and_lists() {
    let edited = edit_settings(
        "",
        "files.ansi-encoding",
        Some(&Value::String("windows-1251".into())),
    )
    .unwrap();
    assert!(
        edited.contains("ansi-encoding = \"windows-1251\""),
        "{edited}"
    );
    let columns = Value::Array(vec![int(80), int(120)]);
    let edited = edit_settings("", "editor.edge-columns", Some(&columns)).unwrap();
    let table: Table = edited.parse().unwrap();
    assert_eq!(table["editor"]["edge-columns"], columns);
    let edited = edit_settings("", "updates.mode", Some(&Value::String("notify".into()))).unwrap();
    assert!(edited.contains("mode = \"notify\""), "{edited}");
}

#[test]
fn removing_goes_back_to_the_default() {
    let text = "[editor]\ntab-width = 2\nword-wrap = true\n";
    let edited = edit_settings(text, "editor.tab-width", None).unwrap();
    assert_eq!(edited, "[editor]\nword-wrap = true\n");
    // Removing what is not there changes nothing.
    assert_eq!(
        edit_settings(text, "editor.auto-indent", None).unwrap(),
        text
    );
    assert_eq!(
        edit_settings(text, "files.recent-limit", None).unwrap(),
        text
    );
    assert_eq!(edit_settings("", "files.recent-limit", None).unwrap(), "");
}

#[test]
fn refuses_unknown_keys_and_wrong_values() {
    for (key, value) in [
        ("editor.no-such", int(1)),
        ("nothing", int(1)),
        ("", int(1)),
        ("editor", int(1)),
    ] {
        assert!(edit_settings("", key, Some(&value)).is_err(), "{key}");
    }
    for (key, value) in [
        ("editor.tab-width", Value::String("four".into())),
        ("editor.tab-width", int(-1)),
        ("editor.tab-width", int(256)),
        ("editor.auto-indent", Value::String("sideways".into())),
        ("files.recent-limit", Value::Boolean(true)),
    ] {
        let error = edit_settings("", key, Some(&value)).unwrap_err();
        assert!(
            matches!(error, SettingError::InvalidValue { .. }),
            "{key}: {error}"
        );
    }
    // Boundaries of a byte: 0 and 255 read.
    assert!(edit_settings("", "editor.tab-width", Some(&int(255))).is_ok());
    assert!(edit_settings("", "editor.tab-width", Some(&int(0))).is_ok());
}

#[test]
fn refuses_files_it_cannot_edit() {
    let error = edit_settings("[editor", "editor.tab-width", Some(&int(2))).unwrap_err();
    assert!(matches!(error, SettingError::Malformed(_)));
    let error = edit_settings("editor = 3\n", "editor.tab-width", Some(&int(2))).unwrap_err();
    assert_eq!(
        error,
        SettingError::NotATable {
            key: "editor".into()
        }
    );
    assert!(!error.to_string().is_empty());
}

#[test]
fn writes_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Birchpad").join("settings.toml");
    write_setting(&path, "editor.tab-width", Some(&int(2))).unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "[editor]\ntab-width = 2\n"
    );
    write_setting(&path, "editor.tab-width", None).unwrap();
    let table: Table = std::fs::read_to_string(&path).unwrap().parse().unwrap();
    assert!(
        table
            .get("editor")
            .is_none_or(|editor| editor.get("tab-width").is_none())
    );
    // A folder in the place of the file.
    let error = write_setting(dir.path(), "editor.tab-width", Some(&int(2))).unwrap_err();
    assert!(matches!(error, SettingError::Io(_)));
    // A file that is not TOML is left alone.
    std::fs::write(&path, "[editor").unwrap();
    assert!(write_setting(&path, "editor.tab-width", Some(&int(2))).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "[editor");
}
