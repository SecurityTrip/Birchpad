//! Settings > Preferences..., as in Notepad++: the settings of `settings.toml` by section, each
//! change written to the file at once (keeping its comments) and applied to the open documents.
//! Settings an administrator's policy sets are shown but cannot be changed.

use std::collections::HashMap;

use anyhow::{Result, anyhow, bail};
use birchpad_config::Settings;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::radio::RadioGroup;
use gpui_kit::component::{Disableable as _, WindowExt as _};
use gpui_kit::{App, AppContext as _, Context, Entity, WeakEntity, Window, div, prelude::*, px};
use toml::Value;

use crate::app_state::AppState;
use crate::commands::CommandRegistry;
use crate::theme::{paint, ui};
use crate::workspace::Workspace;

/// How a setting is edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
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
}

/// A setting the dialog shows.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Entry {
    pub(crate) section: &'static str,
    /// The dotted key in `settings.toml`.
    pub(crate) key: &'static str,
    pub(crate) label: &'static str,
    pub(crate) kind: Kind,
}

const fn entry(section: &'static str, key: &'static str, label: &'static str, kind: Kind) -> Entry {
    Entry {
        section,
        key,
        label,
        kind,
    }
}

const OFF_ON: &[(&str, &str)] = &[("off", "Off"), ("margin", "In the margin")];

/// Every setting the dialog shows, by section. View > Word Wrap and Show Symbol stay in the
/// View menu, which remembers them.
pub(crate) const ENTRIES: &[Entry] = &[
    entry(
        "Editing",
        "editor.tab-width",
        "Tab size",
        Kind::Number { min: 1, max: 16 },
    ),
    entry(
        "Editing",
        "editor.insert-spaces",
        "Replace tabs by spaces",
        Kind::Bool,
    ),
    entry(
        "Editing",
        "editor.auto-indent",
        "Auto-indent",
        Kind::Choice(&[
            ("off", "None"),
            ("basic", "Basic"),
            ("advanced", "Advanced"),
        ]),
    ),
    entry(
        "Editing",
        "editor.current-line",
        "Current line",
        Kind::Choice(&[
            ("off", "None"),
            ("background", "Background"),
            ("frame", "Frame"),
        ]),
    ),
    entry(
        "Editing",
        "editor.current-line-frame-width",
        "Frame width",
        Kind::Number { min: 1, max: 6 },
    ),
    entry("Margins", "editor.line-numbers", "Line numbers", Kind::Bool),
    entry("Margins", "editor.bookmark-margin", "Bookmarks", Kind::Bool),
    entry("Margins", "editor.fold-margin", "Folding", Kind::Bool),
    entry(
        "Margins",
        "editor.change-history",
        "Change history",
        Kind::Choice(OFF_ON),
    ),
    entry(
        "Margins",
        "editor.edge",
        "Vertical edge",
        Kind::Choice(&[
            ("off", "None"),
            ("line", "Line"),
            ("background", "Background"),
        ]),
    ),
    entry(
        "Margins",
        "editor.edge-columns",
        "Edge columns",
        Kind::Numbers { min: 1, max: 9999 },
    ),
    entry(
        "Highlighting",
        "highlighting.smart.enabled",
        "Smart highlighting",
        Kind::Bool,
    ),
    entry(
        "Highlighting",
        "highlighting.smart.match-case",
        "Match case",
        Kind::Bool,
    ),
    entry(
        "Highlighting",
        "highlighting.smart.whole-word",
        "Whole word only",
        Kind::Bool,
    ),
    entry(
        "Highlighting",
        "highlighting.smart.use-find-options",
        "Use the find panel's options",
        Kind::Bool,
    ),
    entry(
        "Highlighting",
        "highlighting.token-style.match-case",
        "Style tokens: match case",
        Kind::Bool,
    ),
    entry(
        "Highlighting",
        "highlighting.token-style.whole-word",
        "Style tokens: whole word only",
        Kind::Bool,
    ),
    entry(
        "Files",
        "files.recent-limit",
        "Recent files",
        Kind::Number { min: 0, max: 100 },
    ),
    entry("Files", "files.ansi-encoding", "ANSI encoding", Kind::Text),
    entry(
        "Files",
        "files.large-file-limit-mb",
        "Large file limit (MB)",
        Kind::Number {
            min: 1,
            max: 1_048_576,
        },
    ),
    entry(
        "Files",
        "files.change-detection",
        "Detect changes by other programs",
        Kind::Choice(&[
            ("all", "All files"),
            ("current", "Current file"),
            ("off", "Off"),
        ]),
    ),
    entry(
        "Files",
        "files.reload-silently",
        "Reload without asking",
        Kind::Bool,
    ),
    entry(
        "Files",
        "files.reload-scrolls-to-end",
        "Go to the end after reloading",
        Kind::Bool,
    ),
    entry(
        "Backup",
        "session.remember",
        "Remember the session",
        Kind::Bool,
    ),
    entry(
        "Backup",
        "session.backup-unsaved",
        "Back up unsaved changes",
        Kind::Bool,
    ),
    entry(
        "Backup",
        "session.backup-interval-seconds",
        "Backup every (seconds)",
        Kind::Number { min: 1, max: 3600 },
    ),
    entry(
        "Updates",
        "updates.mode",
        "Updates",
        Kind::Choice(&[("off", "Off"), ("notify", "Notify"), ("auto", "Automatic")]),
    ),
    entry(
        "Updates",
        "updates.channel",
        "Channel",
        Kind::Choice(&[
            ("stable", "Stable"),
            ("beta", "Beta"),
            ("nightly", "Nightly"),
        ]),
    ),
];

/// The sections in the order listed.
pub(crate) fn sections() -> Vec<&'static str> {
    let mut sections: Vec<&str> = Vec::new();
    for entry in ENTRIES {
        if !sections.contains(&entry.section) {
            sections.push(entry.section);
        }
    }
    sections
}

/// The value of `key` in `settings`, if set.
pub(crate) fn current_value(settings: &Settings, key: &str) -> Option<Value> {
    let table = toml::Table::try_from(settings).expect("settings serialize to a table");
    let mut parts = key.split('.');
    let first = table.get(parts.next()?)?.clone();
    parts.try_fold(first, |value, part| value.get(part).cloned())
}

/// The text a field shows for `value`.
pub(crate) fn display(value: Option<&Value>) -> String {
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
pub(crate) fn parse_field(kind: Kind, text: &str) -> Result<Option<Value>> {
    let number = |text: &str, min: i64, max: i64| -> Result<i64> {
        let value: i64 = text
            .trim()
            .parse()
            .map_err(|_| anyhow!("{} is not a whole number", text.trim()))?;
        if !(min..=max).contains(&value) {
            bail!("{value} is not between {min} and {max}");
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
        Kind::Bool | Kind::Choice(_) => bail!("this setting has no text field"),
    }
}

/// Changes a setting: writes it to the user's settings file, reads the settings again and
/// redraws.
pub(crate) fn set(key: &str, value: Option<Value>, cx: &mut App) -> Result<()> {
    let state = AppState::global(cx);
    if state.is_locked(key) {
        bail!("{key} is set by your administrator");
    }
    let path = state
        .paths
        .user_settings
        .clone()
        .ok_or_else(|| anyhow!("there is no settings file to write to"))?;
    birchpad_config::write_setting(&path, key, value.as_ref())?;
    let paths = AppState::global(cx).paths.clone();
    let resolved = birchpad_config::resolve(birchpad_config::load(&paths));
    for diagnostic in &resolved.diagnostics {
        eprintln!("settings ({}): {}", diagnostic.layer, diagnostic.message);
    }
    cx.global_mut::<AppState>().set_settings(resolved);
    cx.refresh_windows();
    Ok(())
}

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("settings.preferences", |_, (), window, cx| {
        open(cx.entity().downgrade(), window, cx);
        Ok(())
    });
}

pub(crate) struct Preferences {
    workspace: WeakEntity<Workspace>,
    section: usize,
    /// The text fields, by key.
    inputs: HashMap<&'static str, Entity<InputState>>,
    /// What went wrong with a setting, by key.
    errors: HashMap<&'static str, String>,
}

/// Opens the dialog.
pub(crate) fn open(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut App) {
    let preferences = cx.new(|cx| Preferences::new(workspace, window, cx));
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .title("Preferences")
            .w(px(720.))
            .child(preferences.clone())
            .footer(DialogFooter::new().child(crate::workspace::dialog_close("Close")))
    });
}

impl Preferences {
    fn new(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = AppState::global(cx).settings.clone();
        let mut inputs = HashMap::new();
        for entry in ENTRIES {
            if matches!(entry.kind, Kind::Bool | Kind::Choice(_)) {
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
        }
    }

    /// Sets `key`, or shows why not.
    fn apply(&mut self, key: &'static str, value: Result<Option<Value>>, cx: &mut Context<Self>) {
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
    fn field_changed(&mut self, entry: Entry, cx: &mut Context<Self>) {
        let text = self.inputs[entry.key].read(cx).value().to_string();
        let result = parse_field(entry.kind, &text);
        self.apply(entry.key, result, cx);
    }

    fn select_section(&mut self, index: usize, cx: &mut Context<Self>) {
        self.section = index;
        cx.notify();
    }
}

impl Render for Preferences {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = ui();
        let sections = sections();
        let state = AppState::global(cx);
        let settings = state.settings.clone();
        let section = sections[self.section.min(sections.len() - 1)];
        let list = sections.iter().enumerate().map(|(index, name)| {
            div()
                .id(("preferences-section", index))
                .px_2()
                .py_1()
                .when(index == self.section, |row| row.bg(paint(colors.selected)))
                .hover(|row| row.bg(paint(colors.hovered)))
                .child(*name)
                .on_click(cx.listener(move |this, _, _, cx| this.select_section(index, cx)))
        });
        let rows = ENTRIES
            .iter()
            .filter(|entry| entry.section == section)
            .enumerate()
            .map(|(index, entry)| {
                let key = entry.key;
                let locked = state.is_locked(key);
                let value = current_value(&settings, key);
                let control = match entry.kind {
                    Kind::Bool => {
                        let checked = value.as_ref().and_then(Value::as_bool).unwrap_or(false);
                        Checkbox::new(("preferences-check", index))
                            .label(entry.label)
                            .checked(checked)
                            .disabled(locked)
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
                            .child(div().w(px(200.)).child(entry.label))
                            .child(
                                RadioGroup::horizontal(("preferences-choice", index))
                                    .children(choices.iter().map(|(_, label)| *label))
                                    .selected_index(selected)
                                    .disabled(locked)
                                    .on_click(cx.listener(move |this, index: &usize, _, cx| {
                                        let value = Value::String(choices[*index].0.to_owned());
                                        this.apply(key, Ok(Some(value)), cx);
                                    })),
                            )
                            .into_any_element()
                    }
                    Kind::Number { .. } | Kind::Numbers { .. } | Kind::Text => div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_3()
                        .child(div().w(px(200.)).child(entry.label))
                        .child(
                            div()
                                .w(px(200.))
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
                                .child("Set by your administrator"),
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
            .child(
                div()
                    .flex_none()
                    .w(px(150.))
                    .border_1()
                    .border_color(paint(colors.border))
                    .children(list),
            )
            .child(div().flex().flex_col().flex_1().gap_3().children(rows))
    }
}

#[cfg(test)]
mod tests;
