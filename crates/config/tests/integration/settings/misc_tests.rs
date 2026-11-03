//! The settings of Preferences > MISC: closing to the tray.

#[test]
fn closing_to_the_tray_is_asked_until_answered() {
    use birchpad_config::{CloseToTray, Settings, check_setting};
    assert_eq!(Settings::default().misc.close_to_tray, CloseToTray::Ask);
    for (value, setting) in [
        ("ask", CloseToTray::Ask),
        ("tray", CloseToTray::Tray),
        ("close", CloseToTray::Close),
    ] {
        let settings: Settings =
            toml::from_str(&format!("[misc]\nclose-to-tray = \"{value}\"\n")).unwrap();
        assert_eq!(settings.misc.close_to_tray, setting);
        check_setting("misc.close-to-tray", &toml::Value::String(value.into())).unwrap();
    }
    for value in ["", "minimize", "Tray"] {
        assert!(
            check_setting("misc.close-to-tray", &toml::Value::String(value.into())).is_err(),
            "{value}"
        );
    }
    assert!(check_setting("misc.close-to-tray", &toml::Value::Boolean(true)).is_err());
}
