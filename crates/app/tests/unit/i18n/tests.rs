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

#[test]
fn a_notepad_translation_is_read_from_the_users_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("german.xml");
    std::fs::write(
        &path,
        r#"<NotepadPlus><Native-Langue name="Deutsch"><Menu><Main>
            <Entries><Item menuId="file" name="&amp;Datei"/></Entries>
            <Commands><Item id="41001" name="&amp;Neu"/></Commands>
        </Main></Menu></Native-Langue></NotepadPlus>"#,
    )
    .unwrap();
    let texts = native_texts(&path.display().to_string()).unwrap();
    assert_eq!(texts.get("menu.file").map(String::as_str), Some("Datei"));
    assert_eq!(
        texts.get("command.file.new").map(String::as_str),
        Some("Neu")
    );
    // No setting, a missing file, and a file that is not a translation: none.
    assert!(native_texts("  ").is_none());
    assert!(native_texts(&dir.path().join("missing.xml").display().to_string()).is_none());
    std::fs::write(&path, "<broken").unwrap();
    assert!(native_texts(&path.display().to_string()).is_none());
}

#[test]
fn every_notepad_command_is_birchpads() {
    for (id, command) in birchpad_config::NATIVE_LANG_COMMANDS {
        assert!(
            birchpad_commands::COMMANDS
                .iter()
                .any(|spec| spec.id == *command),
            "{id}: {command}"
        );
    }
}
