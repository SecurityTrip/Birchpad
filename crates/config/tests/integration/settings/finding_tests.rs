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

#[test]
fn go_to_file_leaves_out_version_control_and_build_folders_by_default() {
    let finder = FinderSettings::default();
    assert_eq!(finder.filters, r"!+\.git !+\node_modules !+\target");
    assert!(!finder.hidden);
    assert_eq!(finder.file_limit(), 50_000);
    assert_eq!(Settings::default().finder, finder);
}

#[test]
fn go_to_file_s_limit_stays_within_its_bounds() {
    for (set, limit) in [
        (0, 1000),
        (999, 1000),
        (1000, 1000),
        (1_000_000, 1_000_000),
        (1_000_001, 1_000_000),
        (u32::MAX, 1_000_000),
    ] {
        let finder = FinderSettings {
            max_files: set,
            ..FinderSettings::default()
        };
        assert_eq!(finder.file_limit(), limit, "{set}");
    }
    let settings: Settings =
        toml::from_str("[finder]\nfilters = \"*.rs\"\nhidden = true\n").unwrap();
    assert_eq!(settings.finder.filters, "*.rs");
    assert!(settings.finder.hidden);
    assert_eq!(settings.finder.max_files, 50_000);
    check_setting("finder.hidden", &toml::Value::Boolean(true)).unwrap();
    check_setting("finder.filters", &toml::Value::String(String::new())).unwrap();
    assert!(check_setting("finder.max-files", &toml::Value::Integer(-1)).is_err());
    assert!(check_setting("finder.hidden", &toml::Value::Integer(1)).is_err());
}

#[test]
fn sticky_scroll_is_off_and_keeps_five_lines_by_default() {
    let editor = EditorSettings::default();
    assert!(!editor.sticky_scroll);
    assert_eq!(editor.sticky_lines(), 5);
    for (set, lines) in [(0, 1), (1, 1), (10, 10), (11, 10), (255, 10)] {
        let editor = EditorSettings {
            sticky_scroll_lines: set,
            ..EditorSettings::default()
        };
        assert_eq!(editor.sticky_lines(), lines, "{set}");
    }
    let settings: Settings =
        toml::from_str("[editor]\nsticky-scroll = true\nsticky-scroll-lines = 3\n").unwrap();
    assert!(settings.editor.sticky_scroll);
    assert_eq!(settings.editor.sticky_lines(), 3);
    assert!(check_setting("editor.sticky-scroll", &toml::Value::Integer(1)).is_err());
    assert!(check_setting("editor.sticky-scroll-lines", &toml::Value::Integer(-1)).is_err());
}
