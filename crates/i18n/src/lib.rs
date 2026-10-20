//! The languages of Birchpad's interface (ADR 0033), for every crate: the application's window
//! and the crates without one, which make their messages in the interface's language.
//!
//! Every text is looked up by key (`t!("menu.file")`) in a locale file, `locales/<code>.yaml`:
//! English (`en.yaml`) has every key, other languages translate them, and a key a translation
//! lacks shows in English. Texts take named arguments (`t!("macros.there-is-no-macro-named",
//! name = name)` for `"there is no macro named {name}"`) and counts (`plural("plural.matches",
//! 3)`), with the plural forms of each language. The keys of a crate without a window start with
//! its name (`io.`, `config.`, `plugin.`, ...); the application's, with the part of the window
//! they belong to.
//!
//! The language is the process's: [`set_language`] sets it for every thread (with the
//! `per-thread` feature, for tests, for the calling thread only). The application takes it from
//! `appearance.language`; `auto` follows the system.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::{Arc, LazyLock};

use yaml_rust2::{Yaml, YamlLoader};

/// The interface languages, by code, with their names in themselves.
pub const LANGUAGES: &[(&str, &str)] = &[("en", "English"), ("de", "Deutsch"), ("ru", "Русский")];

/// The locales built in, by language code; English first.
pub const LOCALES: &[(&str, &str)] = &[
    ("en", include_str!("../locales/en.yaml")),
    ("de", include_str!("../locales/de.yaml")),
    ("ru", include_str!("../locales/ru.yaml")),
];

/// `t!("key")`, or `t!("key", name = value, ...)` for a text with `{name}` in it.
#[macro_export]
macro_rules! t {
    ($key:expr) => {
        $crate::get($key)
    };
    ($key:expr, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::get_with(
            $key,
            &[$((stringify!($name), &$value as &dyn std::fmt::Display)),+],
        )
    };
}

/// A value of a locale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Text(String),
    /// Plural forms by name (`one`, `few`, `many`, `other`), each with `{count}`.
    Plural(HashMap<String, String>),
}

/// A locale: its texts by key.
#[derive(Debug, Default)]
pub struct Locale {
    pub values: HashMap<String, Value>,
}

const PLURAL_FORMS: &[&str] = &["zero", "one", "two", "few", "many", "other"];

/// Reads a locale file.
pub fn parse_locale(text: &str) -> Result<Locale, String> {
    let documents = YamlLoader::load_from_str(text).map_err(|error| error.to_string())?;
    let mut locale = Locale::default();
    let Some(root) = documents.into_iter().next() else {
        return Ok(locale);
    };
    let Yaml::Hash(root) = root else {
        return Err("a locale is a mapping of keys to texts".to_owned());
    };
    for (key, value) in root {
        let key = scalar(&key).ok_or("keys are text")?;
        flatten(&key, value, &mut locale.values)?;
    }
    Ok(locale)
}

fn scalar(value: &Yaml) -> Option<String> {
    match value {
        Yaml::String(text) | Yaml::Real(text) => Some(text.clone()),
        Yaml::Integer(number) => Some(number.to_string()),
        Yaml::Boolean(value) => Some(value.to_string()),
        _ => None,
    }
}

fn flatten(prefix: &str, value: Yaml, out: &mut HashMap<String, Value>) -> Result<(), String> {
    match value {
        Yaml::Hash(map) => {
            let keys: Vec<Option<String>> = map.keys().map(scalar).collect();
            let plural = !map.is_empty()
                && keys.iter().all(|key| {
                    key.as_deref()
                        .is_some_and(|key| PLURAL_FORMS.contains(&key))
                })
                && map.values().all(|value| scalar(value).is_some());
            if plural {
                let forms = map
                    .iter()
                    .filter_map(|(form, text)| Some((scalar(form)?, scalar(text)?)))
                    .collect();
                out.insert(prefix.to_owned(), Value::Plural(forms));
                return Ok(());
            }
            for (key, value) in map {
                let key = scalar(&key).ok_or_else(|| format!("{prefix}: keys are text"))?;
                flatten(&format!("{prefix}.{key}"), value, out)?;
            }
            Ok(())
        }
        other => {
            let text = scalar(&other).ok_or_else(|| format!("{prefix}: not text"))?;
            out.insert(prefix.to_owned(), Value::Text(text));
            Ok(())
        }
    }
}

/// `text` with each `{name}` replaced by what `arg` gives for it; unknown names stay.
fn fill(text: &str, mut arg: impl FnMut(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            out.push_str(&rest[open..]);
            return out;
        };
        match arg(&after[..close]) {
            Some(value) => out.push_str(&value),
            None => out.push_str(&rest[open..open + close + 2]),
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

/// The language in use.
struct Current {
    code: String,
    /// Its locale, when it is not English.
    translation: Option<Arc<Locale>>,
    /// Texts from a Notepad++ translation, which come first (menu names, command titles).
    native: Option<Arc<HashMap<String, String>>>,
}

fn load(code: &str) -> Option<Arc<Locale>> {
    LOCALES
        .iter()
        .find(|(known, _)| *known == code)
        .map(|(code, text)| {
            Arc::new(
                parse_locale(text).unwrap_or_else(|error| panic!("locales/{code}.yaml: {error}")),
            )
        })
}

static ENGLISH: LazyLock<Arc<Locale>> =
    LazyLock::new(|| load("en").expect("the English locale is built in"));

fn english() -> Current {
    Current {
        code: "en".to_owned(),
        translation: None,
        native: None,
    }
}

#[cfg(not(feature = "per-thread"))]
static CURRENT: LazyLock<std::sync::RwLock<Arc<Current>>> =
    LazyLock::new(|| std::sync::RwLock::new(Arc::new(english())));

#[cfg(not(feature = "per-thread"))]
fn current() -> Arc<Current> {
    CURRENT
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

#[cfg(not(feature = "per-thread"))]
fn set_current(new: Current) {
    *CURRENT
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Arc::new(new);
}

#[cfg(feature = "per-thread")]
thread_local! {
    static CURRENT: std::cell::RefCell<Arc<Current>> =
        std::cell::RefCell::new(Arc::new(english()));
}

#[cfg(feature = "per-thread")]
fn current() -> Arc<Current> {
    CURRENT.with(|current| current.borrow().clone())
}

#[cfg(feature = "per-thread")]
fn set_current(new: Current) {
    CURRENT.with(|current| *current.borrow_mut() = Arc::new(new));
}

/// The language code `setting` stands for: itself if known, the system's for `auto`, English
/// otherwise.
pub fn resolve(setting: &str, system: Option<&str>) -> &'static str {
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

/// Uses the language `code` from now on; English for a language there is no locale for.
pub fn set_language(code: &str) {
    let translation = if code == "en" { None } else { load(code) };
    set_current(Current {
        code: code.to_owned(),
        translation,
        native: current().native.clone(),
    });
}

/// Puts `texts` (by key) before the language's own: the menu names and command titles of a
/// Notepad++ translation the user has (`appearance.native-lang`); `None` takes them away.
pub fn set_native_texts(texts: Option<HashMap<String, String>>) {
    let current = current();
    set_current(Current {
        code: current.code.clone(),
        translation: current.translation.clone(),
        native: texts.map(Arc::new),
    });
}

/// The language in use.
pub fn language() -> String {
    current().code.clone()
}

fn lookup(key: &str) -> Option<Value> {
    let current = current();
    if let Some(text) = current.native.as_ref().and_then(|native| native.get(key)) {
        return Some(Value::Text(text.clone()));
    }
    current
        .translation
        .as_ref()
        .and_then(|locale| locale.values.get(key))
        .or_else(|| ENGLISH.values.get(key))
        .cloned()
}

fn text_of(value: Value) -> String {
    match value {
        Value::Text(text) => text,
        Value::Plural(forms) => forms.get("other").cloned().unwrap_or_default(),
    }
}

/// The text of `key`; the key itself if no locale has it.
pub fn get(key: &str) -> String {
    lookup(key).map_or_else(|| key.to_owned(), text_of)
}

/// The text of `key`, or `fallback` if no locale has it (keys made from data: menu items,
/// commands, settings).
pub fn get_or(key: &str, fallback: &str) -> String {
    lookup(key).map_or_else(|| fallback.to_owned(), text_of)
}

/// The text of `key` with its `{name}` arguments filled in.
pub fn get_with(key: &str, args: &[(&str, &dyn Display)]) -> String {
    fill(&get(key), |name| {
        args.iter()
            .find(|(known, _)| *known == name)
            .map(|(_, value)| value.to_string())
    })
}

/// The plural form of `key` for `count`, with `{count}` filled in.
pub fn plural(key: &str, count: usize) -> String {
    let code = language();
    let text = match lookup(key) {
        Some(Value::Plural(forms)) => {
            let form = plural_form(&code, count);
            forms
                .get(form)
                .or_else(|| forms.get("other"))
                .or_else(|| forms.get("many"))
                .cloned()
                .unwrap_or_default()
        }
        Some(Value::Text(text)) => text,
        None => key.to_owned(),
    };
    fill(&text, |name| (name == "count").then(|| count.to_string()))
}

/// `count` and a noun, from the plural forms of `plural.<many>` (`plural.matches`), or in
/// English as given when no locale has them.
pub fn count(count: usize, one: &str, many: &str) -> String {
    let key = format!("plural.{}", slug(many));
    if lookup(&key).is_some() {
        plural(&key, count)
    } else {
        format!("{count} {}", if count == 1 { one } else { many })
    }
}

/// The plural form a language uses for `count` (CLDR's rules).
fn plural_form(code: &str, count: usize) -> &'static str {
    match code {
        "ru" => {
            let (tens, ones) = (count % 100, count % 10);
            if ones == 1 && tens != 11 {
                "one"
            } else if (2..=4).contains(&ones) && !(12..=14).contains(&tens) {
                "few"
            } else {
                "many"
            }
        }
        _ if count == 1 => "one",
        _ => "other",
    }
}

/// An error as the interface shows it: each message of its chain, joined as `{error:#}` joins
/// them. Birchpad's crates make their messages in the interface's language already; those of
/// the system and of other libraries come as they are.
pub fn tr_error(error: &anyhow::Error) -> String {
    error
        .chain()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ")
}

/// A key made from a text: lower case, words joined by `-` (`"Save As..."` → `save-as`).
pub fn slug(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// The system's language, as `ru-RU`.
pub fn system_language() -> Option<String> {
    sys_locale::get_locale()
}
