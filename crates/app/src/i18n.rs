//! The language of the interface (ADR 0033): `birchpad-i18n`, which every crate translates its
//! texts with, and the setting that chooses it, `appearance.language` (`auto` follows the
//! system); `appearance.native-lang` puts the menu names and command titles of the user's own
//! Notepad++ translation before it.

pub(crate) use birchpad_i18n::{count, get_or, set_language, slug, system_language};

/// Takes `appearance.language` and `appearance.native-lang` from the settings.
pub(crate) fn apply(cx: &gpui_kit::App) {
    let appearance = &crate::app_state::AppState::global(cx).settings.appearance;
    set_language(birchpad_i18n::resolve(
        &appearance.language,
        system_language().as_deref(),
    ));
    let native = native_texts(&appearance.native_lang);
    birchpad_i18n::set_native_texts(native);
}

/// The texts of the Notepad++ translation at `path`; none for an empty path, or a file that
/// does not read (said on the standard error).
pub(crate) fn native_texts(path: &str) -> Option<std::collections::HashMap<String, String>> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    let read = std::fs::read_to_string(path)
        .map_err(|error| error.to_string())
        .and_then(|xml| birchpad_config::parse_native_lang(&xml));
    match read {
        Ok(native) => Some(native.texts),
        Err(error) => {
            eprintln!("{path}: {error}");
            None
        }
    }
}

#[cfg(test)]
mod tests;
