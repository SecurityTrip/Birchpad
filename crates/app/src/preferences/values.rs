//! Reading, parsing and writing the settings' values, and applying them to the open documents.

use std::path::Path;

use anyhow::{Result, anyhow, bail};
use birchpad_config::Settings;
use gpui_kit::App;
use toml::Value;

use crate::app_state::AppState;

use super::Kind;
use super::entries::ENTRIES;

/// Whether the setting `key` does anything on this platform: minimizing to the tray does on
/// Windows and macOS only.
pub fn available_here(key: &str) -> bool {
    key != "misc.minimize-to-tray" || crate::tray::SUPPORTED
}

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
        | Kind::ExplorerMenu
        | Kind::PluginSettings => {
            bail!("{}", t!("preferences.this-setting-has-no-text-field"))
        }
    }
}

/// Changes a setting: writes it to the user's settings file, reads the settings again and
/// redraws.
pub fn set(key: &str, value: Option<Value>, cx: &mut App) -> Result<()> {
    let state = AppState::global(cx);
    if state.is_locked(key) {
        bail!(
            "{}",
            t!("preferences.is-set-by-your-administrator", key = key)
        );
    }
    let path = state
        .paths
        .settings_file_for(key)
        .map(Path::to_owned)
        .ok_or_else(|| {
            anyhow!(
                "{}",
                t!("preferences.there-is-no-settings-file-to-write-to")
            )
        })?;
    birchpad_config::write_setting(&path, key, value.as_ref())?;
    let paths = AppState::global(cx).paths.clone();
    let resolved = birchpad_config::resolve(birchpad_config::load(&paths));
    for diagnostic in &resolved.diagnostics {
        eprintln!("settings ({}): {}", diagnostic.layer, diagnostic.message);
    }
    cx.global_mut::<AppState>().set_settings(resolved);
    crate::i18n::apply(cx);
    // Preferences > Appearance > Interface.
    crate::themes::reapply(cx);
    // Preferences > MISC > Minimize to system tray.
    crate::tray::apply(AppState::global(cx).settings.misc.minimize_to_tray);
    cx.refresh_windows();
    Ok(())
}
