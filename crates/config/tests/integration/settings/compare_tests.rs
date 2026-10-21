use birchpad_config::*;

#[test]
fn compare_turns_everything_on_by_default() {
    let compare = CompareSettings::default();
    assert!(compare.align && compare.inline && compare.navigation_bar && compare.recompare);
    assert_eq!(Settings::default().compare, compare);
}

#[test]
fn compare_reads_from_settings_and_refuses_what_is_not_a_switch() {
    let settings: Settings =
        toml::from_str("[compare]\nalign = false\nnavigation-bar = false\n").unwrap();
    assert!(!settings.compare.align && !settings.compare.navigation_bar);
    // What it does not set keeps its default.
    assert!(settings.compare.inline && settings.compare.recompare);
    for key in [
        "compare.align",
        "compare.inline",
        "compare.navigation-bar",
        "compare.recompare",
    ] {
        check_setting(key, &toml::Value::Boolean(false)).unwrap();
        assert!(
            check_setting(key, &toml::Value::String("yes".into())).is_err(),
            "{key}"
        );
    }
    assert!(check_setting("compare.colors", &toml::Value::Boolean(true)).is_err());
}

#[test]
fn hex_rows_stay_within_their_limits() {
    let hex = HexSettings::default();
    assert_eq!((hex.columns, hex.group, hex.uppercase), (16, 1, true));
    assert_eq!(hex.layout(), (16, 1));
    for (columns, group, expected) in [
        (0, 0, (1, 1)),
        (1, 1, (1, 1)),
        (64, 64, (64, 64)),
        (65, 8, (64, 8)),
        (8, 16, (8, 8)),
        (255, 255, (64, 64)),
    ] {
        let hex = HexSettings {
            columns,
            group,
            uppercase: false,
        };
        assert_eq!(hex.layout(), expected, "{columns} {group}");
    }
}
