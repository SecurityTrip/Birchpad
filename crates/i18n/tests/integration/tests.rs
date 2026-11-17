use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, MutexGuard, PoisonError};

use birchpad_i18n::*;
use std::collections::HashMap;

/// Tests take turns with the language: without the `per-thread` feature it is the process's.
static LANGUAGE: Mutex<()> = Mutex::new(());

fn choosing_language() -> MutexGuard<'static, ()> {
    LANGUAGE.lock().unwrap_or_else(PoisonError::into_inner)
}

fn locale(code: &str) -> Locale {
    let text = LOCALES.iter().find(|(known, _)| *known == code).unwrap().1;
    parse_locale(text).unwrap_or_else(|error| panic!("{code}: {error}"))
}

/// The names and multiplicities of the `{name}` arguments of a text.
fn arguments(text: &str) -> BTreeMap<&str, usize> {
    let mut names = BTreeMap::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else { break };
        *names.entry(&after[..close]).or_default() += 1;
        rest = &after[close + 1..];
    }
    names
}

fn check_text(
    key: &str,
    source: &str,
    text: &str,
    equal_to_english: bool,
    shared: &mut BTreeSet<&str>,
    problems: &mut Vec<String>,
) {
    if arguments(source) != arguments(text) {
        problems.push(format!("{key}: placeholders differ from English"));
    }
    if text.trim().is_empty() {
        problems.push(format!("{key}: empty text"));
    }
    if equal_to_english && !shared.remove(key) {
        problems.push(format!("{key}: untranslated"));
    }
}

fn translation_problems(
    english: &Locale,
    translation: &Locale,
    plural_forms: &[&str],
    mut shared: BTreeSet<&str>,
) -> Vec<String> {
    let mut problems = Vec::new();
    for (key, source) in &english.values {
        let Some(translated) = translation.values.get(key) else {
            problems.push(format!("{key}: missing key"));
            continue;
        };
        match (source, translated) {
            (Value::Text(source), Value::Text(text)) => {
                check_text(
                    key,
                    source,
                    text,
                    source == text,
                    &mut shared,
                    &mut problems,
                );
            }
            (Value::Plural(source), Value::Plural(forms)) => {
                for form in plural_forms {
                    let form_key = format!("{key}.{form}");
                    if let Some(text) = forms.get(*form) {
                        let reference = source.get(*form).unwrap_or(&source["other"]);
                        check_text(
                            &form_key,
                            reference,
                            text,
                            source.values().any(|english| english == text),
                            &mut shared,
                            &mut problems,
                        );
                    } else {
                        problems.push(format!("{form_key}: missing plural form"));
                    }
                }
                for form in forms.keys() {
                    if !plural_forms.contains(&form.as_str()) {
                        problems.push(format!("{key}.{form}: unexpected plural form"));
                    }
                }
            }
            _ => problems.push(format!("{key}: text/plural type differs from English")),
        }
    }
    for key in translation.values.keys() {
        if !english.values.contains_key(key) {
            problems.push(format!("{key}: extra key"));
        }
    }
    problems.extend(
        shared
            .into_iter()
            .map(|key| format!("{key}: unused exception")),
    );
    problems.sort();
    problems
}

#[test]
fn languages_resolve() {
    let _language = choosing_language();
    assert_eq!(resolve("ru", None), "ru");
    assert_eq!(resolve("RU", None), "ru");
    assert_eq!(resolve("auto", Some("ru-RU")), "ru");
    assert_eq!(resolve("Auto", Some("ru_RU.UTF-8")), "ru");
    assert_eq!(resolve("auto", Some("fr-FR")), "fr");
    assert_eq!(resolve("auto", Some("ja-JP")), "en", "no Japanese locale");
    assert_eq!(resolve("auto", None), "en");
    assert_eq!(resolve("", None), "en");
    assert_eq!(resolve("klingon", Some("ru")), "en");
}

#[test]
fn german_is_an_interface_language() {
    let _language = choosing_language();
    assert_eq!(resolve("de", None), "de");
    assert_eq!(resolve("auto", Some("uk-UA")), "en", "no Ukrainian locale");
    assert_eq!(resolve("auto", Some("de_AT.UTF-8")), "de");
    set_language("de");
    assert_eq!(get("menu.file"), "Datei");
    assert_eq!(plural("plural.files", 1), "1 Datei");
    assert_eq!(plural("plural.files", 0), "0 Dateien");
    assert_eq!(plural("plural.files", 2), "2 Dateien");
    set_language("en");
}

#[test]
fn french_is_an_interface_language() {
    let _language = choosing_language();
    assert_eq!(resolve("fr", None), "fr");
    assert_eq!(resolve("FR", None), "fr");
    assert_eq!(resolve("auto", Some("fr_CA.UTF-8")), "fr");
    assert_eq!(resolve("auto", Some("fr-BE")), "fr");
    assert!(LANGUAGES.contains(&("fr", "Français")));
    set_language("fr");
    assert_eq!(get("menu.file"), "Fichier");
    assert_eq!(get("common.cancel"), "Annuler");
    // 0 and 1 are singular in French, 2 and more plural.
    assert_eq!(plural("plural.files", 0), "0 fichier");
    assert_eq!(plural("plural.files", 1), "1 fichier");
    assert_eq!(plural("plural.files", 2), "2 fichiers");
    assert_eq!(plural("plural.files", 1000), "1000 fichiers");
    // An argument is filled in as in English.
    assert_eq!(
        get_with("file-commands.already-exists", &[("path", &"a.txt")]),
        "a.txt existe déjà"
    );
    set_language("en");
}

#[test]
fn spanish_follows_the_system_and_uses_its_cardinal_forms() {
    let _language = choosing_language();
    assert_eq!(resolve("es", None), "es");
    assert_eq!(resolve("auto", Some("es_MX.UTF-8")), "es");
    assert_eq!(resolve("auto", Some("es-419")), "es");
    assert_eq!(resolve("espanol", Some("es")), "en");
    set_language("es");
    assert_eq!(get("menu.file"), "Archivo");
    assert_eq!(get("no.such.key"), "no.such.key");
    for (count, expected) in [
        (0, "0 archivos"),
        (1, "1 archivo"),
        (2, "2 archivos"),
        (999_999, "999999 archivos"),
        (1_000_000, "1000000 de archivos"),
        (1_000_001, "1000001 archivos"),
        (2_000_000, "2000000 de archivos"),
    ] {
        assert_eq!(plural("plural.files", count), expected);
    }
    set_language("en");
}

#[test]
fn hex_column_uses_the_same_label_in_every_language() {
    for &(code, _) in LOCALES {
        assert_eq!(
            locale(code).values["hex-editor.hex-column"],
            Value::Text("HEX".into())
        );
    }
}

#[test]
fn a_notepad_translation_comes_first() {
    let _language = choosing_language();
    set_language("ru");
    set_native_texts(Some(HashMap::from([(
        "menu.file".to_owned(),
        "Datei".to_owned(),
    )])));
    assert_eq!(get("menu.file"), "Datei");
    assert_eq!(
        get("menu.edit"),
        "Правка",
        "what it lacks: the language's own"
    );
    // It stays when the language changes, and goes when taken away.
    set_language("en");
    assert_eq!(get("menu.file"), "Datei");
    assert_eq!(get_or("menu.file", "Fallback"), "Datei");
    set_native_texts(None);
    assert_eq!(get("menu.file"), "File");
    set_native_texts(Some(HashMap::new()));
    assert_eq!(
        get("menu.file"),
        "File",
        "an empty translation changes nothing"
    );
    set_native_texts(None);
}

#[test]
fn translates_by_key_and_falls_back_to_english() {
    let _language = choosing_language();
    set_language("ru");
    assert_eq!(language(), "ru");
    assert_eq!(get("menu.file"), "Файл");
    assert_eq!(get("no.such.key"), "no.such.key");
    assert_eq!(get_or("no.such.key", "Fallback"), "Fallback");
    assert_eq!(get_or("menu.file", "Fallback"), "Файл");
    set_language("en");
    assert_eq!(get("menu.file"), "File");
    set_language("xx");
    assert_eq!(
        get("menu.file"),
        "File",
        "an unknown language shows English"
    );
    set_language("en");
}

#[test]
fn fills_named_arguments() {
    let _language = choosing_language();
    set_language("en");
    assert_eq!(
        t!("macros.there-is-no-macro-named", name = "x"),
        "there is no macro named x"
    );
    set_language("ru");
    assert_eq!(
        t!("macros.there-is-no-macro-named", name = "x"),
        "нет макроса с именем x"
    );
    set_language("en");
    // Unknown names stay as written; a brace without its end too. (A key no locale has is
    // its own text.)
    assert_eq!(get_with("{a} {b} {", &[("a", &1)]), "1 {b} {");
}

#[test]
fn counts_in_each_languages_plural_forms() {
    let _language = choosing_language();
    set_language("en");
    assert_eq!(plural("plural.matches", 1), "1 match");
    assert_eq!(plural("plural.matches", 0), "0 matches");
    assert_eq!(
        count(2, "gizmo", "gizmos"),
        "2 gizmos",
        "unknown nouns stay in English"
    );
    set_language("ru");
    for (number, expected) in [
        (0, "0 совпадений"),
        (1, "1 совпадение"),
        (2, "2 совпадения"),
        (4, "4 совпадения"),
        (5, "5 совпадений"),
        (11, "11 совпадений"),
        (12, "12 совпадений"),
        (21, "21 совпадение"),
        (22, "22 совпадения"),
        (101, "101 совпадение"),
        (111, "111 совпадений"),
    ] {
        assert_eq!(count(number, "match", "matches"), expected);
    }
    assert_eq!(
        plural("menu.file", 3),
        "Файл",
        "a plain text is its own plural"
    );
    assert_eq!(plural("no.such", 3), "no.such");
    set_language("en");
}

#[test]
fn errors_show_their_whole_chain() {
    let _language = choosing_language();
    let error = anyhow::anyhow!("the search text is empty").context("cannot search");
    assert_eq!(tr_error(&error), "cannot search: the search text is empty");
    assert_eq!(
        tr_error(&anyhow::anyhow!("")),
        "",
        "an empty message stays empty"
    );
}

#[cfg(feature = "per-thread")]
#[test]
fn the_language_of_one_thread_is_its_own_in_tests() {
    let _language = choosing_language();
    set_language("ru");
    let other = std::thread::spawn(|| (language(), get("menu.file")))
        .join()
        .unwrap();
    assert_eq!(other, ("en".to_owned(), "File".to_owned()));
    assert_eq!(language(), "ru");
    set_language("en");
}

#[test]
fn refuses_what_is_not_a_locale() {
    let _language = choosing_language();
    for text in [
        "- a\n- b",
        "a: [1, 2]",
        "a: {b: [1]}",
        "text:\n  [1]: x",
        "\"a: b",
    ] {
        assert!(parse_locale(text).is_err(), "{text}");
    }
    assert!(parse_locale("").unwrap().values.is_empty());
    let locale = parse_locale("a:\n  b: 1\n  c: true\nn:\n  one: x\n  other: y").unwrap();
    assert_eq!(locale.values["a.b"], Value::Text("1".into()));
    assert_eq!(locale.values["a.c"], Value::Text("true".into()));
    assert!(matches!(locale.values["n"], Value::Plural(_)));
}

#[test]
fn makes_keys_from_texts() {
    let _language = choosing_language();
    assert_eq!(slug("Save As..."), "save-as");
    assert_eq!(slug("  Ln : 3 "), "ln-3");
    assert_eq!(slug("Мир"), "мир");
    assert_eq!(slug("..."), "");
}

#[test]
fn every_translation_has_exactly_the_english_keys_and_matching_placeholders() {
    let english = locale("en");
    let exceptions: Vec<_> = include_str!("shared-with-english.txt")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (key, codes) = line.split_once(' ').unwrap();
            for code in codes.split_whitespace() {
                assert_ne!(code, "en", "{key}: English needs no exception");
                assert!(
                    LOCALES.iter().any(|(known, _)| *known == code),
                    "{key}: {code}"
                );
            }
            (key, codes)
        })
        .collect();
    let mut all_problems = Vec::new();
    for &(code, _) in LOCALES.iter().filter(|(code, _)| *code != "en") {
        let translation = locale(code);
        let shared = exceptions
            .iter()
            .filter(|(_, codes)| codes.split_whitespace().any(|entry| entry == code))
            .map(|(key, _)| *key)
            .collect();
        let forms: &[_] = match code {
            "ru" => &["one", "few", "many"],
            "es" => &["one", "many", "other"],
            _ => &["one", "other"],
        };
        all_problems.extend(
            translation_problems(&english, &translation, forms, shared)
                .into_iter()
                .map(|problem| format!("{code}: {problem}")),
        );
    }
    assert!(all_problems.is_empty(), "{all_problems:#?}");
}

#[test]
fn translation_check_reports_missing_extra_english_and_broken_placeholders() {
    let english = parse_locale(
        "a: 'Open {path} ({path})'\nb: Open\nc: OK\nd: Save\ne: {one: '{count} file', other: '{count} files'}",
    ).unwrap();
    let translation = parse_locale(
        "a: 'Öffne {path}'\nb: Open\nc: OK\ne: {one: '{count} Datei', many: '{count} Dateien'}\nextra: Mehr",
    ).unwrap();
    assert_eq!(
        translation_problems(
            &english,
            &translation,
            &["one", "other"],
            BTreeSet::from(["c"])
        ),
        [
            "a: placeholders differ from English",
            "b: untranslated",
            "d: missing key",
            "e.many: unexpected plural form",
            "e.other: missing plural form",
            "extra: extra key",
        ],
    );
    let empty = Locale::default();
    assert!(translation_problems(&empty, &empty, &[], BTreeSet::new()).is_empty());
}

/// Every `t!("key")` of the code of every crate has its text in en.yaml.
#[test]
fn every_key_in_the_code_is_in_english() {
    let _language = choosing_language();
    let english = locale("en");
    let mut missing = Vec::new();
    fn visit(dir: &std::path::Path, found: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, found);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                let mut rest = text.as_str();
                while let Some(at) = rest.find("t!(\"") {
                    let before = rest[..at].chars().next_back();
                    let after = &rest[at + 4..];
                    let end = after.find('"').unwrap();
                    if !before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                        found.push((path.display().to_string(), after[..end].to_owned()));
                    }
                    rest = &after[end..];
                }
            }
        }
    }
    let mut found = Vec::new();
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    for entry in std::fs::read_dir(crates).unwrap() {
        let source = entry.unwrap().path().join("src");
        if source.is_dir() {
            visit(&source, &mut found);
        }
    }
    assert!(found.len() > 200, "{}", found.len());
    for (file, key) in found {
        if file.ends_with("tests.rs") || file.contains("i18n") || key.contains(' ') {
            continue;
        }
        if !english.values.contains_key(&key) {
            missing.push(format!("{file}: {key}"));
        }
    }
    assert!(missing.is_empty(), "{missing:#?}");
}
