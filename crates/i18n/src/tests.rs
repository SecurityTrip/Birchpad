use std::collections::BTreeSet;

use super::*;

fn locale(code: &str) -> Locale {
    let text = LOCALES.iter().find(|(known, _)| *known == code).unwrap().1;
    parse_locale(text).unwrap_or_else(|error| panic!("{code}: {error}"))
}

/// The names of the `{name}` arguments of a text.
fn arguments(text: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else { break };
        names.insert(after[..close].to_owned());
        rest = &after[close + 1..];
    }
    names
}

fn texts(value: &Value) -> Vec<&String> {
    match value {
        Value::Text(text) => vec![text],
        Value::Plural(forms) => forms.values().collect(),
    }
}

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
fn translates_by_key_and_falls_back_to_english() {
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
    // Unknown names stay as written; a brace without its end too.
    assert_eq!(
        fill("{a} {b} {", |name| (name == "a").then(|| "1".into())),
        "1 {b} {"
    );
}

#[test]
fn counts_in_each_languages_plural_forms() {
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
    let error = anyhow::anyhow!("the search text is empty").context("cannot search");
    assert_eq!(tr_error(&error), "cannot search: the search text is empty");
    assert_eq!(
        tr_error(&anyhow::anyhow!("")),
        "",
        "an empty message stays empty"
    );
}

#[test]
fn the_language_of_one_thread_is_its_own_in_tests() {
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
    assert_eq!(slug("Save As..."), "save-as");
    assert_eq!(slug("  Ln : 3 "), "ln-3");
    assert_eq!(slug("Мир"), "мир");
    assert_eq!(slug("..."), "");
}

#[test]
fn every_language_has_every_key_with_its_arguments() {
    let english = locale("en");
    for (code, _) in LOCALES {
        let translation = locale(code);
        for (key, value) in &english.values {
            let translated = translation
                .values
                .get(key)
                .unwrap_or_else(|| panic!("{code}: {key} is not translated"));
            let wanted: BTreeSet<String> = texts(value).iter().flat_map(|t| arguments(t)).collect();
            for text in texts(translated) {
                assert_eq!(arguments(text), wanted, "{code}: {key} = {text}");
            }
        }
        for key in translation.values.keys() {
            assert!(
                english.values.contains_key(key),
                "{code}: {key} is not in en.yaml"
            );
        }
    }
}

/// Every `t!("key")` of the code of every crate has its text in en.yaml.
#[test]
fn every_key_in_the_code_is_in_english() {
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
