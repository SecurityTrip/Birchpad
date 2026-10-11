//! The language of the interface (ADR 0033): English, as the code writes it, or a translation
//! from `locales/<code>.toml`, which maps each English text to its translation. Text without a
//! translation stays in English. `appearance.language` chooses it: `auto` follows the system.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// The interface languages, by code, with their names in themselves.
pub(crate) const LANGUAGES: &[(&str, &str)] = &[("en", "English"), ("ru", "Русский")];

/// The translations built in, by language code.
const CATALOGS: &[(&str, &str)] = &[("ru", include_str!("../locales/ru.toml"))];

/// English text to its translation.
type Catalog = Rc<HashMap<String, String>>;

thread_local! {
    /// The language in use and its catalog (none for English).
    static CURRENT: RefCell<(String, Option<Catalog>)> = RefCell::new(("en".to_owned(), None));
}

/// The language code `setting` stands for: itself if known, the system's for `auto`, English
/// otherwise.
pub(crate) fn resolve(setting: &str, system: Option<&str>) -> &'static str {
    let wanted = if setting.eq_ignore_ascii_case("auto") {
        system.unwrap_or("en")
    } else {
        setting
    };
    // `ru-RU`, `ru_RU.UTF-8`: the language part.
    let code = wanted
        .split(['-', '_', '.'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    LANGUAGES
        .iter()
        .find(|(known, _)| *known == code)
        .map_or("en", |(known, _)| known)
}

/// Reads a catalog: a TOML table of strings.
pub(crate) fn parse_catalog(text: &str) -> Result<HashMap<String, String>, String> {
    let table: toml::Table = toml::from_str(text).map_err(|error| error.message().to_owned())?;
    table
        .into_iter()
        .map(|(key, value)| match value {
            toml::Value::String(value) => Ok((key, value)),
            other => Err(format!("{key}: {other} is not text")),
        })
        .collect()
}

/// Uses the language `code` from now on.
pub(crate) fn set_language(code: &str) {
    let catalog = CATALOGS
        .iter()
        .find(|(known, _)| *known == code)
        .map(|(_, text)| Rc::new(parse_catalog(text).expect("built-in catalogs read")));
    CURRENT.with(|current| *current.borrow_mut() = (code.to_owned(), catalog));
}

/// The language in use.
#[cfg(test)]
pub(crate) fn language() -> String {
    CURRENT.with(|current| current.borrow().0.clone())
}

/// `text` in the interface's language.
pub(crate) fn tr(text: &str) -> String {
    CURRENT.with(|current| {
        current
            .borrow()
            .1
            .as_ref()
            .and_then(|catalog| catalog.get(text).cloned())
            .unwrap_or_else(|| text.to_owned())
    })
}

/// The system's language, as `ru-RU`.
pub(crate) fn system_language() -> Option<String> {
    sys_locale::get_locale()
}

/// Takes `appearance.language` from the settings.
pub(crate) fn apply(cx: &gpui_kit::App) {
    let setting = crate::app_state::AppState::global(cx)
        .settings
        .appearance
        .language
        .clone();
    set_language(resolve(&setting, system_language().as_deref()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_resolve() {
        assert_eq!(resolve("ru", None), "ru");
        assert_eq!(resolve("RU", None), "ru");
        assert_eq!(resolve("auto", Some("ru-RU")), "ru");
        assert_eq!(resolve("Auto", Some("ru_RU.UTF-8")), "ru");
        assert_eq!(resolve("auto", Some("de-DE")), "en");
        assert_eq!(resolve("auto", None), "en");
        assert_eq!(resolve("", None), "en");
        assert_eq!(resolve("klingon", Some("ru")), "en");
    }

    #[test]
    fn translates_and_falls_back() {
        set_language("ru");
        assert_eq!(language(), "ru");
        assert_eq!(tr("File"), "Файл");
        assert_eq!(tr("Not translated"), "Not translated");
        assert_eq!(tr(""), "");
        set_language("en");
        assert_eq!(tr("File"), "File");
        set_language("xx");
        assert_eq!(tr("File"), "File");
        set_language("en");
    }

    #[test]
    fn catalogs_read_and_cover_the_menus() {
        for (code, text) in CATALOGS {
            let catalog = parse_catalog(text).unwrap_or_else(|error| panic!("{code}: {error}"));
            for menu in birchpad_commands::main_menu() {
                assert!(catalog.contains_key(&menu.title), "{code}: {}", menu.title);
                for invocation in menu.invocations() {
                    let title = birchpad_commands::find(&invocation.command).unwrap().title;
                    if invocation.args.is_null() {
                        assert!(catalog.contains_key(title), "{code}: {title}");
                    }
                }
            }
        }
        assert!(parse_catalog("a = 1").is_err());
        assert!(parse_catalog("[a").is_err());
        assert!(parse_catalog("").unwrap().is_empty());
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
            assert_eq!(tr("Preferences..."), "Настройки...");
            cx.global_mut::<crate::app_state::AppState>()
                .settings
                .appearance
                .language = "en".into();
            apply(cx);
            assert_eq!(tr("Preferences..."), "Preferences...");
        });
    }
}
