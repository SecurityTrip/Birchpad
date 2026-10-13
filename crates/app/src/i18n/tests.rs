use super::*;
use birchpad_i18n::get;

fn english() -> birchpad_i18n::Locale {
    birchpad_i18n::parse_locale(birchpad_i18n::LOCALES[0].1).unwrap()
}

#[test]
fn every_command_menu_and_panel_has_its_text() {
    let english = english();
    let mut missing = Vec::new();
    let mut check = |key: String, text: &str| {
        // Numbers and names of encodings need no translation.
        let words = text.chars().any(|c| c.is_lowercase());
        if words
            && !english.values.contains_key(&key)
            && !missing.contains(&format!("{key} = {text}"))
        {
            missing.push(format!("{key} = {text}"));
        }
    };
    for spec in birchpad_commands::COMMANDS {
        check(crate::menus::command_key(spec.id), spec.title);
    }
    fn walk(menu: &birchpad_commands::Menu, check: &mut dyn FnMut(String, &str)) {
        check(crate::menus::menu_key(&menu.title), &menu.title);
        for item in &menu.items {
            match item {
                birchpad_commands::MenuItem::Command {
                    label: Some(label), ..
                } => check(crate::menus::menu_key(label), label),
                birchpad_commands::MenuItem::Submenu(sub) => walk(sub, check),
                _ => {}
            }
        }
    }
    for menu in birchpad_commands::main_menu() {
        walk(&menu, &mut check);
    }
    for kind in crate::panels::PanelKind::ALL {
        check(format!("panel.{}", kind.id()), kind.title());
    }
    assert!(missing.is_empty(), "{missing:#?}");
}

#[gpui_kit::test]
fn the_setting_chooses_the_language(cx: &mut gpui_kit::TestAppContext) {
    let (_, cx) = crate::workspace::tests::open_workspace(cx);
    cx.update(|_, cx| {
        cx.global_mut::<crate::app_state::AppState>()
            .settings
            .appearance
            .language = "ru".into();
        apply(cx);
        assert_eq!(get("command.settings.preferences"), "Настройки...");
        cx.global_mut::<crate::app_state::AppState>()
            .settings
            .appearance
            .language = "en".into();
        apply(cx);
        assert_eq!(get("command.settings.preferences"), "Preferences...");
    });
}
