//! The settings of the previews: the background of PlantUML diagrams.

use birchpad_config::{DiagramBackground, Settings, check_setting};

#[test]
fn diagrams_are_drawn_on_white_by_default() {
    let settings = Settings::default();
    assert_eq!(
        settings.preview.plantuml_background,
        DiagramBackground::White
    );
    assert_eq!(settings.preview.plantuml, "plantuml");
}

#[test]
fn the_diagram_background_reads_both_values_and_nothing_else() {
    let settings: Settings =
        toml::from_str("[preview]\nplantuml-background = \"theme\"\n").unwrap();
    assert_eq!(
        settings.preview.plantuml_background,
        DiagramBackground::Theme
    );
    assert_eq!(
        settings.preview.plantuml, "plantuml",
        "the rest as it comes"
    );
    for value in ["white", "theme"] {
        check_setting(
            "preview.plantuml-background",
            &toml::Value::String(value.into()),
        )
        .unwrap();
    }
    for value in ["black", "", "White"] {
        assert!(
            check_setting(
                "preview.plantuml-background",
                &toml::Value::String(value.into())
            )
            .is_err(),
            "{value}"
        );
    }
    assert!(check_setting("preview.plantuml-background", &toml::Value::Boolean(true)).is_err());
    assert!(check_setting("preview.background", &toml::Value::String("white".into())).is_err());
}

#[test]
fn diagrams_are_exported_at_twice_their_size_by_default() {
    assert_eq!(Settings::default().preview.plantuml_png_scale, 2);
    let settings: Settings = toml::from_str("[preview]\nplantuml-png-scale = 8\n").unwrap();
    assert_eq!(settings.preview.plantuml_png_scale, 8);
    check_setting("preview.plantuml-png-scale", &toml::Value::Integer(1)).unwrap();
    assert!(
        check_setting(
            "preview.plantuml-png-scale",
            &toml::Value::String("2".into())
        )
        .is_err()
    );
    assert!(check_setting("preview.plantuml-png-scale", &toml::Value::Integer(-1)).is_err());
}
