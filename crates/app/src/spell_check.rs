//! Plugins > Spell Checker's commands of the window (ADR 0039): checking automatically, the
//! dictionary's language and its folder. The checking itself, and the commands that run in an
//! editor, are `birchpad-spell-check`'s.

use anyhow::{Context as _, anyhow, bail};
pub use birchpad_spell_check::*;
use serde::Deserialize;

use crate::app_state::AppState;
use crate::commands::{CommandRegistry, Register as _};

#[derive(Deserialize)]
struct LanguageArgs {
    language: String,
}

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("spell.auto", |this, (), _, cx| {
        let on = !is_enabled(cx);
        if on {
            speller(cx)?;
        }
        AppState::update_state(cx, |state, _| state.spell_check.enabled = on);
        redraw(cx);
        this.refresh_menus(cx);
        Ok(())
    });
    registry.workspace("spell.language", |this, args: LanguageArgs, _, cx| {
        let language = args.language.trim().to_owned();
        if language.is_empty() || language.contains(['/', '\\', '.']) {
            bail!(
                "{}",
                t!(
                    "spell-check.is-not-a-dictionary-s-language",
                    language = format!("{:?}", args.language)
                )
            );
        }
        AppState::update_state(cx, |state, _| state.spell_check.language = Some(language));
        forget(cx);
        this.refresh_menus(cx);
        speller(cx).map(|_| ())
    });
    registry.workspace("spell.open-dictionaries-folder", |_, (), _, cx| {
        let dir = dictionaries_dir(cx)
            .ok_or_else(|| anyhow!("{}", t!("spell-check.there-is-no-settings-folder")))?;
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("cannot create {}", dir.display()))?;
        cx.open_with_system(&dir);
        forget(cx);
        Ok(())
    });
    birchpad_spell_check::register_commands(registry);
}
