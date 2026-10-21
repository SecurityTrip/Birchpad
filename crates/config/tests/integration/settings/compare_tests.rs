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
