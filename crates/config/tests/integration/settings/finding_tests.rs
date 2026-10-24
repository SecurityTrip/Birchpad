use birchpad_config::*;

#[test]
fn the_palette_lists_eight_recent_commands_by_default() {
    let palette = PaletteSettings::default();
    assert_eq!(palette.recent_commands, 8);
    assert_eq!(Settings::default().palette, palette);
    assert_eq!(palette.recent_limit(), 8);
}

#[test]
fn the_palette_s_recent_commands_stay_within_their_limit() {
    for (set, limit) in [(0, 0), (1, 1), (50, 50), (51, 50), (255, 50)] {
        let palette = PaletteSettings {
            recent_commands: set,
        };
        assert_eq!(palette.recent_limit(), limit, "{set}");
    }
    let settings: Settings = toml::from_str("[palette]\nrecent-commands = 3\n").unwrap();
    assert_eq!(settings.palette.recent_commands, 3);
    check_setting("palette.recent-commands", &toml::Value::Integer(0)).unwrap();
    assert!(check_setting("palette.recent-commands", &toml::Value::Integer(-1)).is_err());
    assert!(check_setting("palette.recent-commands", &toml::Value::Integer(256)).is_err());
    assert!(check_setting("palette.recent-commands", &toml::Value::Boolean(true)).is_err());
}
