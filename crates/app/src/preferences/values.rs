//! Writing the settings' values, and applying them to the open documents.

use std::path::Path;

use anyhow::{Result, anyhow, bail};
use gpui_kit::App;
use toml::Value;

use crate::app_state::AppState;

/// Whether the setting `key` does anything on this platform: minimizing to the tray does on
/// Windows and macOS only.
pub fn available_here(key: &str) -> bool {
    !matches!(key, "misc.minimize-to-tray" | "misc.close-to-tray") || crate::tray::SUPPORTED
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
    // Preferences > Updates > Send install statistics.
    crate::updates::forget_install_id_when_off(cx);
    crate::i18n::apply(cx);
    // Preferences > Appearance > Interface.
    crate::themes::reapply(cx);
    // Preferences > MISC > Minimize to system tray.
    crate::tray::apply(AppState::global(cx).settings.misc.minimize_to_tray);
    cx.refresh_windows();
    Ok(())
}
