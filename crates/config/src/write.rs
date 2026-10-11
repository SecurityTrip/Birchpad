//! Changing one setting in the user's `settings.toml`, as Preferences does: the rest of the
//! file, its comments and its layout stay as they are.

use std::path::Path;

use toml::{Table, Value};

use crate::layers::{Sources, resolve};
use crate::settings::Settings;

/// Why a setting could not be changed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SettingError {
    #[error("there is no setting {0}")]
    UnknownKey(String),
    #[error("{key}: {message}")]
    InvalidValue { key: String, message: String },
    #[error("the settings file is not valid TOML: {0}")]
    Malformed(String),
    #[error("{key} is in the way: it is not a table in the settings file")]
    NotATable { key: String },
    #[error("cannot write the settings: {0}")]
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
    if known.is_none() && !optional.contains(&key) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn int(value: i64) -> Value {
        Value::Integer(value)
    }

    #[test]
    fn sets_a_key_keeping_the_rest() {
        let text =
            "# My settings\n[editor]\n# four is too much\ntab-width = 4 # old\nword-wrap = true\n";
        let edited = edit_settings(text, "editor.tab-width", Some(&int(2))).unwrap();
        assert_eq!(
            edited,
            "# My settings\n[editor]\n# four is too much\ntab-width = 2 # old\nword-wrap = true\n"
        );
        // A new key goes into its table, which is made if needed.
        let edited =
            edit_settings(text, "editor.insert-spaces", Some(&Value::Boolean(true))).unwrap();
        assert!(edited.contains("insert-spaces = true"), "{edited}");
        let edited = edit_settings(
            "",
            "highlighting.smart.enabled",
            Some(&Value::Boolean(false)),
        )
        .unwrap();
        let table: Table = edited.parse().unwrap();
        assert_eq!(
            table["highlighting"]["smart"]["enabled"],
            Value::Boolean(false)
        );
        // Dotted keys and inline tables are tables too.
        let edited =
            edit_settings("editor.tab-width = 8\n", "editor.tab-width", Some(&int(3))).unwrap();
        assert_eq!(edited, "editor.tab-width = 3\n");
        let edited = edit_settings(
            "editor = { tab-width = 8 }\n",
            "editor.tab-width",
            Some(&int(3)),
        )
        .unwrap();
        assert_eq!(edited, "editor = { tab-width = 3 }\n");
    }

    #[test]
    fn strings_and_lists() {
        let edited = edit_settings(
            "",
            "files.ansi-encoding",
            Some(&Value::String("windows-1251".into())),
        )
        .unwrap();
        assert!(
            edited.contains("ansi-encoding = \"windows-1251\""),
            "{edited}"
        );
        let columns = Value::Array(vec![int(80), int(120)]);
        let edited = edit_settings("", "editor.edge-columns", Some(&columns)).unwrap();
        let table: Table = edited.parse().unwrap();
        assert_eq!(table["editor"]["edge-columns"], columns);
        let edited =
            edit_settings("", "updates.mode", Some(&Value::String("notify".into()))).unwrap();
        assert!(edited.contains("mode = \"notify\""), "{edited}");
    }

    #[test]
    fn removing_goes_back_to_the_default() {
        let text = "[editor]\ntab-width = 2\nword-wrap = true\n";
        let edited = edit_settings(text, "editor.tab-width", None).unwrap();
        assert_eq!(edited, "[editor]\nword-wrap = true\n");
        // Removing what is not there changes nothing.
        assert_eq!(
            edit_settings(text, "editor.auto-indent", None).unwrap(),
            text
        );
        assert_eq!(
            edit_settings(text, "files.recent-limit", None).unwrap(),
            text
        );
        assert_eq!(edit_settings("", "files.recent-limit", None).unwrap(), "");
    }

    #[test]
    fn refuses_unknown_keys_and_wrong_values() {
        for (key, value) in [
            ("editor.no-such", int(1)),
            ("nothing", int(1)),
            ("", int(1)),
            ("editor", int(1)),
        ] {
            assert!(edit_settings("", key, Some(&value)).is_err(), "{key}");
        }
        for (key, value) in [
            ("editor.tab-width", Value::String("four".into())),
            ("editor.tab-width", int(-1)),
            ("editor.tab-width", int(256)),
            ("editor.auto-indent", Value::String("sideways".into())),
            ("files.recent-limit", Value::Boolean(true)),
        ] {
            let error = edit_settings("", key, Some(&value)).unwrap_err();
            assert!(
                matches!(error, SettingError::InvalidValue { .. }),
                "{key}: {error}"
            );
        }
        // Boundaries of a byte: 0 and 255 read.
        assert!(edit_settings("", "editor.tab-width", Some(&int(255))).is_ok());
        assert!(edit_settings("", "editor.tab-width", Some(&int(0))).is_ok());
    }

    #[test]
    fn refuses_files_it_cannot_edit() {
        let error = edit_settings("[editor", "editor.tab-width", Some(&int(2))).unwrap_err();
        assert!(matches!(error, SettingError::Malformed(_)));
        let error = edit_settings("editor = 3\n", "editor.tab-width", Some(&int(2))).unwrap_err();
        assert_eq!(
            error,
            SettingError::NotATable {
                key: "editor".into()
            }
        );
        assert!(!error.to_string().is_empty());
    }

    #[test]
    fn writes_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Birchpad").join("settings.toml");
        write_setting(&path, "editor.tab-width", Some(&int(2))).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[editor]\ntab-width = 2\n"
        );
        write_setting(&path, "editor.tab-width", None).unwrap();
        let table: Table = std::fs::read_to_string(&path).unwrap().parse().unwrap();
        assert!(
            table
                .get("editor")
                .is_none_or(|editor| editor.get("tab-width").is_none())
        );
        // A folder in the place of the file.
        let error = write_setting(dir.path(), "editor.tab-width", Some(&int(2))).unwrap_err();
        assert!(matches!(error, SettingError::Io(_)));
        // A file that is not TOML is left alone.
        std::fs::write(&path, "[editor").unwrap();
        assert!(write_setting(&path, "editor.tab-width", Some(&int(2))).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[editor");
    }
}
