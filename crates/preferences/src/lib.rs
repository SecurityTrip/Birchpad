//! What Settings > Preferences... lists, without a window: every setting the dialog shows, by
//! section ([`ENTRIES`]), how each is edited ([`Kind`]), and their values as the text of a field
//! ([`current_value`], [`display`], [`parse_field`]). The dialog itself is the application's.

#[macro_use]
extern crate birchpad_i18n;

mod entries;

pub use entries::ENTRIES;

use anyhow::{Result, anyhow, bail};
use birchpad_config::Settings;
use toml::Value;

/// How a setting is edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Bool,
    /// A whole number from `min` to `max`.
    Number {
        min: i64,
        max: i64,
    },
    /// One of these values, with their labels.
    Choice(&'static [(&'static str, &'static str)]),
    /// Text; empty removes the setting.
    Text,
    /// Whole numbers separated by commas or spaces.
    Numbers {
        min: i64,
        max: i64,
    },
    /// Preferences > Language: tab size and spaces of each language (`languages`).
    LanguageTabs,
    /// Preferences > Language Servers: the server of each language (`lsp.servers`).
    LanguageServers,
    /// "Edit with Birchpad" in the Windows Explorer's menu of files, for the user: the
    /// registry's, not a setting.
    ExplorerMenu,
    /// Preferences > Plugins: the settings each plugin declares, kept in the plugins folder
    /// (ADR 0046), not in `settings.toml`.
    PluginSettings,
}

/// A setting the dialog shows.
#[derive(Debug, Clone, Copy)]
pub struct Entry {
    pub section: &'static str,
    /// The dotted key in `settings.toml`.
    pub key: &'static str,
    pub label: &'static str,
    pub kind: Kind,
}

pub const fn entry(
    section: &'static str,
    key: &'static str,
    label: &'static str,
    kind: Kind,
) -> Entry {
    Entry {
        section,
        key,
        label,
        kind,
    }
}

/// The key of Preferences > Files > Edit with Birchpad, which is no setting.
pub const EXPLORER_MENU: &str = "explorer-menu";
/// The key of Preferences > Plugins, whose settings are the plugins' own.
pub const PLUGIN_SETTINGS: &str = "plugins.settings";

/// The sections in the order listed.
pub fn sections() -> Vec<&'static str> {
    let mut sections: Vec<&str> = Vec::new();
    for entry in ENTRIES {
        if !sections.contains(&entry.section) {
            sections.push(entry.section);
        }
    }
    sections
}

/// The value of `key` in `settings`, if set.
pub fn current_value(settings: &Settings, key: &str) -> Option<Value> {
    let table = toml::Table::try_from(settings).expect("settings serialize to a table");
    let mut parts = key.split('.');
    let first = table.get(parts.next()?)?.clone();
    parts.try_fold(first, |value, part| value.get(part).cloned())
}

/// The text a field shows for `value`.
pub fn display(value: Option<&Value>) -> String {
    match value {
        None => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| display(Some(item)))
            .collect::<Vec<_>>()
            .join(", "),
        Some(value) => value.to_string(),
    }
}

/// Reads what was typed into the field of a setting of `kind`. `None`: remove the setting.
pub fn parse_field(kind: Kind, text: &str) -> Result<Option<Value>> {
    let number = |text: &str, min: i64, max: i64| -> Result<i64> {
        let value: i64 = text.trim().parse().map_err(|_| {
            anyhow!(
                "{}",
                t!("preferences.is-not-a-whole-number", text = text.trim())
            )
        })?;
        if !(min..=max).contains(&value) {
            bail!(
                "{}",
                t!(
                    "preferences.is-not-between-and",
                    value = value,
                    min = min,
                    max = max
                )
            );
        }
        Ok(value)
    };
    match kind {
        Kind::Number { min, max } => Ok(Some(Value::Integer(number(text, min, max)?))),
        Kind::Numbers { min, max } => {
            let values = text
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter(|part| !part.is_empty())
                .map(|part| number(part, min, max).map(Value::Integer))
                .collect::<Result<Vec<_>>>()?;
            Ok(Some(Value::Array(values)))
        }
        Kind::Text => {
            let text = text.trim();
            Ok((!text.is_empty()).then(|| Value::String(text.to_owned())))
        }
        Kind::Bool
        | Kind::Choice(_)
        | Kind::LanguageTabs
        | Kind::LanguageServers
        | Kind::ExplorerMenu
        | Kind::PluginSettings => {
            bail!("{}", t!("preferences.this-setting-has-no-text-field"))
        }
    }
}
