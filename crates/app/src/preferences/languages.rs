//! Preferences > Language: each language's own tab size and spaces.

use anyhow::{Result, anyhow};
use gpui_kit::component::button::Button;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::Input;
use gpui_kit::component::{Disableable as _, Sizable as _};
use gpui_kit::{AnyElement, Context, div, prelude::*, px};
use toml::Value;

use crate::app_state::AppState;

use super::values::{parse_field, set};
use super::{Kind, Preferences, label};

impl Preferences {
    /// Preferences > Language > Set: the language's own tab size and spaces.
    pub fn set_language_tabs(&mut self, cx: &mut Context<Self>) {
        let typed = self.language_input.read(cx).value().to_string();
        let width = self.language_width.read(cx).value().to_string();
        let result = language_id(&typed).and_then(|id| {
            let width = parse_field(Kind::Number { min: 1, max: 16 }, &width)?;
            set(&format!("languages.{id}.tab-width"), width, cx)?;
            set(
                &format!("languages.{id}.insert-spaces"),
                Some(Value::Boolean(self.language_spaces)),
                cx,
            )
        });
        self.after_language_change(result, cx);
    }

    /// Preferences > Language > Remove: the language takes the editor's tab settings again.
    pub fn remove_language_tabs(&mut self, id: &str, cx: &mut Context<Self>) {
        let result = set(&format!("languages.{id}.tab-width"), None, cx)
            .and_then(|()| set(&format!("languages.{id}.insert-spaces"), None, cx));
        self.after_language_change(result, cx);
    }

    pub(super) fn after_language_change(&mut self, result: Result<()>, cx: &mut Context<Self>) {
        match result {
            Ok(()) => {
                self.errors.remove("languages");
                self.workspace
                    .update(cx, |workspace, cx| workspace.refresh_views(cx))
                    .ok();
            }
            Err(error) => {
                self.errors.insert("languages", format!("{error:#}"));
            }
        }
        cx.notify();
    }

    /// The language table: one row per language with its own settings, then the fields that
    /// set one.
    pub(super) fn render_language_tabs(&self, locked: bool, cx: &mut Context<Self>) -> AnyElement {
        let settings = AppState::global(cx).settings.languages.clone();
        let rows = settings
            .iter()
            .filter(|(_, own)| own.tab_width.is_some() || own.insert_spaces.is_some())
            .enumerate()
            .map(|(index, (id, own))| {
                let id = id.clone();
                let describe = format!(
                    "{id}: {}{}",
                    own.tab_width
                        .map_or_else(String::new, |width| width.to_string()),
                    match own.insert_spaces {
                        Some(true) => format!(", {}", t!("preferences.spaces")),
                        Some(false) => format!(", {}", t!("preferences.tabs")),
                        None => String::new(),
                    }
                );
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(div().w(px(260.)).child(describe))
                    .child(
                        Button::new(("language-remove", index))
                            .debug_selector(move || format!("language-remove-{index}"))
                            .small()
                            .label(t!("common.remove"))
                            .disabled(locked)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.remove_language_tabs(&id, cx);
                            })),
                    )
            });
        let spaces = self.language_spaces;
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(label("Tab settings"))
            .children(rows)
            // Wrapped: on one line the fields are wider than the dialog.
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .w(px(160.))
                            .debug_selector(|| "language-id".into())
                            .child(Input::new(&self.language_input)),
                    )
                    .child(
                        div()
                            .w(px(60.))
                            .debug_selector(|| "language-width".into())
                            .child(Input::new(&self.language_width)),
                    )
                    .child(
                        Checkbox::new("language-spaces")
                            .debug_selector(|| "language-spaces".into())
                            .label(label("Replace tabs by spaces"))
                            .checked(spaces)
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                this.language_spaces = *checked;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("language-set")
                            .debug_selector(|| "language-set".into())
                            .small()
                            .label(t!("preferences.set"))
                            .disabled(locked)
                            .on_click(cx.listener(|this, _, _, cx| this.set_language_tabs(cx))),
                    ),
            )
            .into_any_element()
    }
}

/// The id of a language as typed: its id or its name, without regard to case.
pub fn language_id(typed: &str) -> Result<String> {
    let typed = typed.trim();
    if typed.eq_ignore_ascii_case("text") || typed.eq_ignore_ascii_case("normal text") {
        return Ok("text".to_owned());
    }
    birchpad_syntax::LANGUAGES
        .iter()
        .find(|language| {
            language.id.eq_ignore_ascii_case(typed) || language.name.eq_ignore_ascii_case(typed)
        })
        .map(|language| language.id.to_owned())
        .ok_or_else(|| anyhow!("{}", t!("preferences.there-is-no-language", name = typed)))
}
