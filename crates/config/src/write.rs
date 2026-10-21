//! Changing one setting in the user's `settings.toml`, as Preferences does: the rest of the
//! file, its comments and its layout stay as they are.

use std::path::Path;

use toml::{Table, Value};

use crate::layers::{Sources, resolve};
use crate::settings::Settings;

/// Why a setting could not be changed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SettingError {
    #[error("{}", birchpad_i18n::t!("config.no-setting", key = .0))]
    UnknownKey(String),
    #[error("{key}: {message}")]
    InvalidValue { key: String, message: String },
    #[error("{}", birchpad_i18n::t!("config.not-toml", reason = .0))]
    Malformed(String),
    #[error("{}", birchpad_i18n::t!("config.not-a-table", key = .key))]
    NotATable { key: String },
    #[error("{}", birchpad_i18n::t!("config.cannot-write-settings", reason = .0))]
    Io(String),
}

/// Checks that `value` is a valid value of the setting `key` (`editor.tab-width`).
pub fn check_setting(key: &str, value: &Value) -> Result<(), SettingError> {
    let defaults = Table::try_from(Settings::default()).expect("defaults serialize to a table");
    let mut parts = key.split('.');
    let first = parts.next().and_then(|part| defaults.get(part));
    let known = first.and_then(|first| parts.try_fold(first, |value, part| value.get(part)));
    // `files.ansi-encoding` and `updates.url` have no default, and are known all the same.
    let optional = ["files.ansi-encoding", "updates.url"];
    // `languages.<id>.tab-width` and `.insert-spaces`, for any language.
    let language = matches!(
        key.split('.').collect::<Vec<_>>().as_slice(),
        ["languages", id, "tab-width" | "insert-spaces"] if !id.is_empty()
    );
    if known.is_none() && !optional.contains(&key) && !language {
        return Err(SettingError::UnknownKey(key.to_owned()));
    }
    let mut table = Table::new();
    crate::layers::set_path(&mut table, key, value.clone());
    let resolved = resolve(Sources {
        user: Some(table),
        ..Sources::default()
    });
    match resolved.diagnostics.into_iter().next() {
        Some(diagnostic) => Err(SettingError::InvalidValue {
            key: key.to_owned(),
            message: diagnostic.message,
        }),
        None => Ok(()),
    }
}

/// The text of a settings file with `key` set to `value`, or removed when `value` is `None`
/// (the setting goes back to its default).
pub fn edit_settings(text: &str, key: &str, value: Option<&Value>) -> Result<String, SettingError> {
    if let Some(value) = value {
        check_setting(key, value)?;
    }
    let mut document: toml_edit::DocumentMut =
        text.parse().map_err(|error: toml_edit::TomlError| {
            SettingError::Malformed(error.message().to_owned())
        })?;
    if document.as_table().is_empty() && !text.trim().is_empty() {
        // Only comments: they stay on top, where they were written, rather than after what is
        // added.
        let added = edit_settings("", key, value)?;
        if added.is_empty() {
            return Ok(text.to_owned());
        }
        let separator = if text.ends_with('\n') { "" } else { "\n" };
        return Ok(format!("{text}{separator}{added}"));
    }
    let parts: Vec<&str> = key.split('.').collect();
    let (last, tables) = parts.split_last().expect("split yields at least one part");
    let mut table: &mut dyn toml_edit::TableLike = document.as_table_mut();
    for (depth, part) in tables.iter().enumerate() {
        let not_a_table = || SettingError::NotATable {
            key: parts[..=depth].join("."),
        };
        if table.get(part).is_none() {
            if value.is_none() {
                return Ok(document.to_string());
            }
            let mut new = toml_edit::Table::new();
            new.set_implicit(true);
            table.insert(part, toml_edit::Item::Table(new));
        }
        table = table
            .get_mut(part)
            .and_then(toml_edit::Item::as_table_like_mut)
            .ok_or_else(not_a_table)?;
    }
    match value {
        Some(value) => {
            let mut value: toml_edit::Value = value
                .to_string()
                .parse()
                .expect("a TOML value prints as TOML");
            match table.get_mut(last) {
                // In place, keeping the comments around the old value.
                Some(toml_edit::Item::Value(old)) => {
                    *value.decor_mut() = old.decor().clone();
                    *old = value;
                }
                _ => {
                    table.insert(last, toml_edit::Item::Value(value));
                }
            }
        }
        None => {
            table.remove(last);
        }
    }
    Ok(document.to_string())
}

/// Sets `key` in the user's settings file at `path`, creating it if needed.
pub fn write_setting(path: &Path, key: &str, value: Option<&Value>) -> Result<(), SettingError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(SettingError::Io(format!("{}: {error}", path.display()))),
    };
    let text = edit_settings(&text, key, value)?;
    crate::session::write_private(path, text.as_bytes())
        .map_err(|error| SettingError::Io(format!("{}: {error}", path.display())))
}
