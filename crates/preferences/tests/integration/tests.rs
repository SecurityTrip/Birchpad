use birchpad_config::Settings;
use birchpad_preferences::*;
use toml::Value;

#[test]
fn every_entry_is_a_setting_with_valid_choices() {
    let settings = Settings::default();
    let mut keys = std::collections::HashSet::new();
    for entry in ENTRIES {
        assert!(keys.insert(entry.key), "{} twice", entry.key);
        // Neither is a setting of settings.toml.
        if matches!(entry.kind, Kind::ExplorerMenu | Kind::PluginSettings) {
            assert!(birchpad_config::check_setting(entry.key, &Value::Boolean(true)).is_err());
            continue;
        }
        // Every setting but the optional text ones has a value to show.
        let value = current_value(&settings, entry.key);
        if entry.kind != Kind::Text {
            let value = value.unwrap_or_else(|| panic!("{} has no value", entry.key));
            birchpad_config::check_setting(entry.key, &value).unwrap();
        }
        if let Kind::Choice(choices) = entry.kind {
            for (choice, _) in choices {
                birchpad_config::check_setting(entry.key, &Value::String((*choice).into()))
                    .unwrap_or_else(|error| panic!("{error}"));
            }
        }
        if let Kind::Number { min, max } = entry.kind {
            for value in [min, max] {
                birchpad_config::check_setting(entry.key, &Value::Integer(value)).unwrap();
            }
        }
    }
    assert_eq!(
        sections(),
        [
            "Appearance",
            "Toolbar",
            "Tab Bar",
            "New Document",
            "Language",
            "Language Servers",
            "Git",
            "Terminal",
            "Default Directory",
            "Multi-Instance & Date",
            "Cloud & Link",
            "Delimiter",
            "Search Engine",
            "Editing",
            "Auto-Completion",
            "Margins",
            "Highlighting",
            "Print",
            "Preview",
            "Files",
            "Searching",
            "Backup",
            "Go to File",
            "Command Palette",
            "Performance",
            "MISC",
            "Spell Checker",
            "Hex Editor",
            "Compare",
            "Plugins",
            "Updates"
        ]
    );
}

#[test]
fn values_show_as_text() {
    let settings = Settings::default();
    assert_eq!(
        display(current_value(&settings, "editor.tab-width").as_ref()),
        "4"
    );
    assert_eq!(
        display(current_value(&settings, "editor.edge-columns").as_ref()),
        "80"
    );
    assert_eq!(
        display(current_value(&settings, "updates.mode").as_ref()),
        "auto"
    );
    assert_eq!(display(None), "");
    assert_eq!(
        display(Some(&Value::Array(vec![
            Value::Integer(80),
            Value::Integer(120)
        ]))),
        "80, 120"
    );
    assert_eq!(current_value(&settings, "files.ansi-encoding"), None);
    assert_eq!(current_value(&settings, "no.such"), None);
    assert_eq!(current_value(&settings, ""), None);
}

#[test]
fn fields_read_numbers_lists_and_text() {
    let number = Kind::Number { min: 1, max: 16 };
    assert_eq!(parse_field(number, " 8 ").unwrap(), Some(Value::Integer(8)));
    assert_eq!(parse_field(number, "1").unwrap(), Some(Value::Integer(1)));
    assert_eq!(parse_field(number, "16").unwrap(), Some(Value::Integer(16)));
    for text in ["0", "17", "", "four", "4.5", "-1", "99999999999999999999"] {
        assert!(parse_field(number, text).is_err(), "{text:?}");
    }
    let numbers = Kind::Numbers { min: 1, max: 9999 };
    assert_eq!(
        parse_field(numbers, "80, 120 160").unwrap(),
        Some(Value::Array(vec![
            Value::Integer(80),
            Value::Integer(120),
            Value::Integer(160)
        ]))
    );
    assert_eq!(
        parse_field(numbers, "").unwrap(),
        Some(Value::Array(vec![]))
    );
    assert!(parse_field(numbers, "80, x").is_err());
    assert!(parse_field(numbers, "0").is_err());
    assert_eq!(
        parse_field(Kind::Text, " windows-1251 ").unwrap(),
        Some(Value::String("windows-1251".into()))
    );
    assert_eq!(parse_field(Kind::Text, "  ").unwrap(), None);
    assert!(parse_field(Kind::Bool, "true").is_err());
    assert!(parse_field(Kind::Choice(&[]), "x").is_err());
}

#[test]
fn entries_made_at_run_time() {
    let made = entry("Toolbar", "toolbar.show", "Show the toolbar", Kind::Bool);
    assert_eq!(
        (made.section, made.key, made.label),
        ("Toolbar", "toolbar.show", "Show the toolbar")
    );
    assert!(ENTRIES.iter().any(|known| known.key == made.key));
}
