//! The language of the interface (ADR 0033).
//!
//! Every text the interface shows is looked up by key (`t!("menu.file")`) in a locale file,
//! `locales/<code>.yaml`: English (`en.yaml`) has every key, other languages translate them,
//! and a key a translation lacks shows in English. Texts take named arguments
//! (`t!("macros.no-macro-named", name = name)` for `"there is no macro named {name}"`) and
//! counts (`plural("plural.match", 3)`), with the plural forms of each language.
//!
//! Errors of the crates without a window (`birchpad-io`, `birchpad-config`, ...) are English
//! sentences; a locale translates them in its `text` section, English text to translation,
//! where `{}` stands for any part (`"cannot read {}: {}"`).
//!
//! `appearance.language` chooses the language: `auto` follows the system.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Display;
use std::rc::Rc;

use yaml_rust2::{Yaml, YamlLoader};

/// The interface languages, by code, with their names in themselves.
pub(crate) const LANGUAGES: &[(&str, &str)] = &[("en", "English"), ("ru", "Русский")];

/// The locales built in, by language code; English first.
const LOCALES: &[(&str, &str)] = &[
    ("en", include_str!("../locales/en.yaml")),
    ("ru", include_str!("../locales/ru.yaml")),
];

/// `t!("key")`, or `t!("key", name = value, ...)` for a text with `{name}` in it.
macro_rules! t {
    ($key:expr) => {
        $crate::i18n::get($key)
    };
    ($key:expr, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::get_with(
            $key,
            &[$((stringify!($name), &$value as &dyn std::fmt::Display)),+],
        )
    };
}

/// A value of a locale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Value {
    Text(String),
    /// Plural forms by name (`one`, `few`, `many`, `other`), each with `{count}`.
    Plural(HashMap<String, String>),
}

/// A locale: its texts by key, and its translations of English sentences.
#[derive(Debug, Default)]
pub(crate) struct Locale {
    pub(crate) values: HashMap<String, Value>,
    /// The `text` section: English sentences, with `{}` for any part, split at it, with their
    /// translations; the longest first, so that the most specific one wins.
    sentences: Vec<(Vec<String>, String)>,
    exact_sentences: HashMap<String, String>,
}

const PLURAL_FORMS: &[&str] = &["zero", "one", "two", "few", "many", "other"];

/// Reads a locale file.
pub(crate) fn parse_locale(text: &str) -> Result<Locale, String> {
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
        if key == "text" {
            let Yaml::Hash(sentences) = value else {
                return Err("text: a mapping of English sentences to translations".to_owned());
            };
            for (english, translation) in sentences {
                let english = scalar(&english).ok_or("text: keys are text")?;
                let translation =
                    scalar(&translation).ok_or_else(|| format!("text: {english}: not text"))?;
                locale.add_sentence(english, translation);
            }
        } else {
            flatten(&key, value, &mut locale.values)?;
        }
    }
    locale.sentences.sort_by_key(|(pieces, _)| {
        std::cmp::Reverse(pieces.iter().map(String::len).sum::<usize>())
    });
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

impl Locale {
    fn add_sentence(&mut self, english: String, translation: String) {
        if english.contains("{}") {
            let pieces = english.split("{}").map(str::to_owned).collect();
            self.sentences.push((pieces, translation));
        } else {
            self.exact_sentences.insert(english, translation);
        }
    }

    /// The translation of an English sentence, if this locale has one.
    fn sentence(&self, text: &str, depth: usize) -> Option<String> {
        if let Some(found) = self.exact_sentences.get(text) {
            return Some(found.clone());
        }
        if depth == 0 {
            return None;
        }
        self.sentences.iter().find_map(|(pieces, translation)| {
            let args: Vec<String> = matches(pieces, text)?
                .into_iter()
                .map(|arg| {
                    // A sentence inside a sentence (a cause) is translated too; a name is not.
                    if arg.contains(' ') {
                        self.sentence(arg, depth - 1)
                            .unwrap_or_else(|| arg.to_owned())
                    } else {
                        arg.to_owned()
                    }
                })
                .collect();
            Some(fill_positional(translation, &args))
        })
    }
}

/// What stands for each `{}` when `text` is made of `pieces` (a sentence split at `{}`).
fn matches<'a>(pieces: &[String], text: &'a str) -> Option<Vec<&'a str>> {
    let (first, rest) = pieces.split_first()?;
    let mut remaining = text.strip_prefix(first.as_str())?;
    let mut args = Vec::with_capacity(rest.len());
    for (index, piece) in rest.iter().enumerate() {
        let at = if index == rest.len() - 1 {
            if !remaining.ends_with(piece.as_str()) {
                return None;
            }
            remaining.len() - piece.len()
        } else if piece.is_empty() {
            // Two `{}` in a row cannot be told apart.
            return None;
        } else {
            remaining.find(piece.as_str())?
        };
        let arg = &remaining[..at];
        if arg.is_empty() {
            return None;
        }
        args.push(arg);
        remaining = &remaining[at + piece.len()..];
    }
    Some(args)
}

/// `translation` with `args` in its `{}` (in order) or `{1}`, `{2}`, ...
fn fill_positional(translation: &str, args: &[String]) -> String {
    let mut next = 0;
    fill(translation, |name| {
        if name.is_empty() {
            next += 1;
            args.get(next - 1).cloned()
        } else {
            let index: usize = name.parse().ok()?;
            args.get(index.checked_sub(1)?).cloned()
        }
    })
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

struct Current {
    code: String,
    english: Rc<Locale>,
    /// The language in use, when it is not English.
    translation: Option<Rc<Locale>>,
}

fn load(code: &str) -> Option<Rc<Locale>> {
    LOCALES
        .iter()
        .find(|(known, _)| *known == code)
        .map(|(code, text)| {
            Rc::new(
                parse_locale(text).unwrap_or_else(|error| panic!("locales/{code}.yaml: {error}")),
            )
        })
}

thread_local! {
    static CURRENT: RefCell<Current> = RefCell::new(Current {
        code: "en".to_owned(),
        english: load("en").expect("the English locale is built in"),
        translation: None,
    });
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

/// Uses the language `code` from now on.
pub(crate) fn set_language(code: &str) {
    let translation = if code == "en" { None } else { load(code) };
    CURRENT.with(|current| {
        let mut current = current.borrow_mut();
        current.code = code.to_owned();
        current.translation = translation;
    });
}

/// The language in use.
#[cfg(test)]
pub(crate) fn language() -> String {
    CURRENT.with(|current| current.borrow().code.clone())
}

fn lookup(key: &str) -> Option<Value> {
    CURRENT.with(|current| {
        let current = current.borrow();
        current
            .translation
            .as_ref()
            .and_then(|locale| locale.values.get(key))
            .or_else(|| current.english.values.get(key))
            .cloned()
    })
}

fn text_of(value: Value) -> String {
    match value {
        Value::Text(text) => text,
        Value::Plural(forms) => forms.get("other").cloned().unwrap_or_default(),
    }
}

/// The text of `key`; the key itself if no locale has it.
pub(crate) fn get(key: &str) -> String {
    lookup(key).map_or_else(|| key.to_owned(), text_of)
}

/// The text of `key`, or `fallback` if no locale has it (keys made from data: menu items,
/// commands, settings).
pub(crate) fn get_or(key: &str, fallback: &str) -> String {
    lookup(key).map_or_else(|| fallback.to_owned(), text_of)
}

/// The text of `key` with its `{name}` arguments filled in.
pub(crate) fn get_with(key: &str, args: &[(&str, &dyn Display)]) -> String {
    fill(&get(key), |name| {
        args.iter()
            .find(|(known, _)| *known == name)
            .map(|(_, value)| value.to_string())
    })
}

/// The plural form of `key` for `count`, with `{count}` filled in.
pub(crate) fn plural(key: &str, count: usize) -> String {
    let code = CURRENT.with(|current| current.borrow().code.clone());
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
pub(crate) fn count(count: usize, one: &str, many: &str) -> String {
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

/// An English sentence from a crate without a window, in the interface's language.
pub(crate) fn tr_text(text: &str) -> String {
    CURRENT.with(|current| {
        current
            .borrow()
            .translation
            .as_ref()
            .and_then(|locale| locale.sentence(text, 3))
            .unwrap_or_else(|| text.to_owned())
    })
}

/// An error in the interface's language: each message of its chain, joined as `{error:#}`
/// joins them. Messages of the application are made in it already; those of the other crates
/// are translated as [`tr_text`].
pub(crate) fn tr_error(error: &anyhow::Error) -> String {
    error
        .chain()
        .map(|cause| tr_text(&cause.to_string()))
        .collect::<Vec<_>>()
        .join(": ")
}

/// A key made from a text: lower case, words joined by `-` (`"Save As..."` → `save-as`).
pub(crate) fn slug(text: &str) -> String {
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
mod tests;
