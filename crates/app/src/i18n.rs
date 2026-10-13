//! The language of the interface (ADR 0033): `birchpad-i18n`, which every crate translates its
//! texts with, and the setting that chooses it, `appearance.language` (`auto` follows the
//! system).

pub(crate) use birchpad_i18n::{count, get_or, set_language, slug, system_language, tr_error};

/// Takes `appearance.language` from the settings.
pub(crate) fn apply(cx: &gpui_kit::App) {
    let setting = crate::app_state::AppState::global(cx)
        .settings
        .appearance
        .language
        .clone();
    set_language(birchpad_i18n::resolve(
        &setting,
        system_language().as_deref(),
    ));
}

#[cfg(test)]
mod tests;
