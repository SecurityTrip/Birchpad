//! Settings > Preferences..., as in Notepad++: the settings of `settings.toml` by section, each
//! change written to the file at once (keeping its comments) and applied to the open documents.
//! Settings an administrator's policy sets are shown but cannot be changed.

mod entries;
mod languages;
mod plugins;
mod values;

pub use entries::ENTRIES;
pub use languages::language_id;
pub use plugins::plugin_settings;
pub use values::{available_here, current_value, display, parse_field, sections, set};

use std::collections::HashMap;

use anyhow::Result;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::radio::RadioGroup;
use gpui_kit::component::{Disableable as _, WindowExt as _};
use gpui_kit::{App, AppContext as _, Context, Entity, WeakEntity, Window, div, prelude::*, px};
use toml::Value;

use crate::app_state::AppState;
use crate::commands::{CommandRegistry, Register as _};
use crate::theme::{paint, ui};
use crate::workspace::Workspace;

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

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("settings.preferences", |_, (), window, cx| {
        open(cx.entity().downgrade(), window, cx);
        Ok(())
    });
}

pub struct Preferences {
    workspace: WeakEntity<Workspace>,
    pub section: usize,
    /// The text fields, by key.
    pub inputs: HashMap<&'static str, Entity<InputState>>,
    /// What went wrong with a setting, by key.
    pub errors: HashMap<&'static str, String>,
    /// Preferences > Language: the language and the tab size to set.
    language_input: Entity<InputState>,
    language_width: Entity<InputState>,
    language_spaces: bool,
    /// Preferences > Plugins: the fields of the plugins' settings, by plugin and key.
    pub plugin_inputs: HashMap<(String, String), Entity<InputState>>,
    /// What went wrong with a plugin's setting, by plugin and key.
    pub plugin_errors: HashMap<(String, String), String>,
}

/// Opens the dialog.
pub fn open(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut App) {
    open_at(workspace, None, window, cx);
}

/// Opens the dialog at `section` (`Print`), or at the first one.
pub fn open_at(
    workspace: WeakEntity<Workspace>,
    section: Option<&str>,
    window: &mut Window,
    cx: &mut App,
) {
    let preferences = cx.new(|cx| {
        let mut preferences = Preferences::new(workspace, window, cx);
        if let Some(index) =
            section.and_then(|section| sections().iter().position(|known| *known == section))
        {
            preferences.section = index;
        }
        preferences
    });
    #[cfg(feature = "test-support")]
    OPENED.with(|opened| *opened.borrow_mut() = Some(preferences.downgrade()));
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .title(t!("preferences.preferences"))
            .w(px(720.))
            .child(preferences.clone())
            .footer(DialogFooter::new().child(crate::workspace::dialog_close("common.close")))
    });
}

#[cfg(feature = "test-support")]
thread_local! {
    /// The dialog last opened, for tests to drive.
    pub static OPENED: std::cell::RefCell<Option<WeakEntity<Preferences>>> =
        std::cell::RefCell::default();
}

impl Preferences {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings = AppState::global(cx).settings.clone();
        let mut inputs = HashMap::new();
        for entry in ENTRIES {
            if matches!(
                entry.kind,
                Kind::Bool
                    | Kind::Choice(_)
                    | Kind::LanguageTabs
                    | Kind::ExplorerMenu
                    | Kind::PluginSettings
            ) {
                continue;
            }
            let value = display(current_value(&settings, entry.key).as_ref());
            let input = cx.new(|cx| InputState::new(window, cx).default_value(value));
            let entry = *entry;
            cx.subscribe(&input, move |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.field_changed(entry, cx);
                }
            })
            .detach();
            inputs.insert(entry.key, input);
        }
        Self {
            workspace,
            section: 0,
            inputs,
            errors: HashMap::new(),
            language_input: cx
                .new(|cx| InputState::new(window, cx).placeholder(t!("preferences.language-id"))),
            language_width: cx.new(|cx| InputState::new(window, cx).default_value("4")),
            language_spaces: false,
            plugin_inputs: HashMap::new(),
            plugin_errors: HashMap::new(),
        }
    }

    /// Sets `key`, or shows why not.
    pub fn apply(
        &mut self,
        key: &'static str,
        value: Result<Option<Value>>,
        cx: &mut Context<Self>,
    ) {
        match value.and_then(|value| set(key, value, cx)) {
            Ok(()) => {
                self.errors.remove(key);
                self.workspace
                    .update(cx, |workspace, cx| workspace.refresh_views(cx))
                    .ok();
            }
            Err(error) => {
                self.errors.insert(key, format!("{error:#}"));
            }
        }
        cx.notify();
    }

    /// The text of a field changed.
    pub fn field_changed(&mut self, entry: Entry, cx: &mut Context<Self>) {
        let text = self.inputs[entry.key].read(cx).value().to_string();
        let result = parse_field(entry.kind, &text);
        self.apply(entry.key, result, cx);
    }

    /// Preferences > Files: "Edit with Birchpad" in the Explorer's menu, for this copy.
    pub fn switch_explorer_menu(&mut self, on: bool, cx: &mut Context<Self>) {
        let result = if on {
            std::env::current_exe().and_then(|exe| birchpad_cli::explorer::add(&exe))
        } else {
            birchpad_cli::explorer::remove()
        };
        match result {
            Ok(()) => {
                self.errors.remove(EXPLORER_MENU);
            }
            Err(error) => {
                self.errors.insert(
                    EXPLORER_MENU,
                    t!(
                        "preferences.cannot-change-the-explorer-s-menu",
                        error = error
                    ),
                );
            }
        }
        cx.notify();
    }

    pub fn select_section(&mut self, index: usize, cx: &mut Context<Self>) {
        self.section = index;
        cx.notify();
    }
}

/// A section's, a setting's or a choice's text: `preferences.<text>`.
fn label(text: &str) -> String {
    crate::i18n::get_or(&format!("preferences.{}", crate::i18n::slug(text)), text)
}

impl Render for Preferences {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = ui();
        let sections = sections();
        let state = AppState::global(cx);
        let settings = state.settings.clone();
        let locked_keys: Vec<&str> = ENTRIES
            .iter()
            .map(|entry| entry.key)
            .filter(|key| state.is_locked(key))
            .collect();
        let section = sections[self.section.min(sections.len() - 1)];
        let mut language_tabs = (section == "Language")
            .then(|| self.render_language_tabs(locked_keys.contains(&"languages"), cx));
        let mut plugin_settings = (section == "Plugins").then(|| {
            self.make_plugin_inputs(window, cx);
            self.render_plugin_settings(cx)
        });
        let list = sections.iter().enumerate().map(|(index, name)| {
            div()
                .id(("preferences-section", index))
                .debug_selector(move || format!("preferences-section-{index}"))
                .px_2()
                .py_1()
                .when(index == self.section, |row| row.bg(paint(colors.selected)))
                .hover(|row| row.bg(paint(colors.hovered)))
                .child(label(name))
                .on_click(cx.listener(move |this, _, _, cx| this.select_section(index, cx)))
        });
        let rows = ENTRIES
            .iter()
            .filter(|entry| entry.section == section)
            .enumerate()
            .map(|(index, entry)| {
                let key = entry.key;
                let locked = locked_keys.contains(&key);
                let value = current_value(&settings, key);
                let control = match entry.kind {
                    Kind::Bool => {
                        let checked = value.as_ref().and_then(Value::as_bool).unwrap_or(false);
                        Checkbox::new(("preferences-check", index))
                            .debug_selector(move || format!("preferences-{key}"))
                            .label(label(entry.label))
                            .checked(checked)
                            .disabled(locked || !available_here(key))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.apply(key, Ok(Some(Value::Boolean(!checked))), cx);
                            }))
                            .into_any_element()
                    }
                    Kind::Choice(choices) => {
                        let current = value.as_ref().and_then(Value::as_str).unwrap_or_default();
                        let selected = choices.iter().position(|(value, _)| *value == current);
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_3()
                            .child(div().w(px(200.)).child(label(entry.label)))
                            .child(
                                div()
                                    .debug_selector(move || format!("preferences-{key}"))
                                    .child(
                                        RadioGroup::horizontal(("preferences-choice", index))
                                            .children(choices.iter().map(|(_, text)| label(text)))
                                            .selected_index(selected)
                                            .disabled(locked)
                                            .on_click(cx.listener(
                                                move |this, index: &usize, _, cx| {
                                                    let value =
                                                        Value::String(choices[*index].0.to_owned());
                                                    this.apply(key, Ok(Some(value)), cx);
                                                },
                                            )),
                                    ),
                            )
                            .into_any_element()
                    }
                    Kind::ExplorerMenu => {
                        let added = birchpad_cli::explorer::is_added();
                        Checkbox::new(("preferences-check", index))
                            .debug_selector(move || format!("preferences-{key}"))
                            .label(label(entry.label))
                            .checked(added)
                            .disabled(!cfg!(windows))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.switch_explorer_menu(!added, cx);
                            }))
                            .into_any_element()
                    }
                    Kind::LanguageTabs => language_tabs
                        .take()
                        .unwrap_or_else(|| div().into_any_element()),
                    Kind::PluginSettings => plugin_settings
                        .take()
                        .unwrap_or_else(|| div().into_any_element()),
                    Kind::Number { .. } | Kind::Numbers { .. } | Kind::Text => div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_3()
                        .child(div().w(px(200.)).child(label(entry.label)))
                        .child(
                            div()
                                .w(px(200.))
                                .debug_selector(move || format!("preferences-{key}"))
                                .child(Input::new(&self.inputs[key]).disabled(locked)),
                        )
                        .into_any_element(),
                };
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(control)
                    .when(locked, |row| {
                        row.child(
                            div()
                                .text_sm()
                                .text_color(paint(colors.muted))
                                .child(t!("preferences.set-by-your-administrator")),
                        )
                    })
                    .children(self.errors.get(key).map(|error| {
                        div()
                            .text_sm()
                            .text_color(paint(colors.error))
                            .child(error.clone())
                    }))
            });
        div()
            .flex()
            .flex_row()
            .gap_4()
            .h(px(380.))
            // Both scroll: there are more sections, and some sections have more rows, than
            // the dialog's height shows.
            .child(
                div()
                    .id("preferences-sections")
                    .flex_none()
                    .w(px(150.))
                    .overflow_y_scroll()
                    .border_1()
                    .border_color(paint(colors.border))
                    .children(list),
            )
            .child(
                div()
                    .id("preferences-rows")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .gap_3()
                    .overflow_y_scroll()
                    .children(rows),
            )
    }
}
