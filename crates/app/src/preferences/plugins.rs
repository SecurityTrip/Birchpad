//! Preferences > Plugins: the settings the installed plugins declare.

use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::{AnyElement, App, AppContext as _, Context, Window, div, prelude::*, px};

use crate::theme::{paint, ui};

use super::{PLUGIN_SETTINGS, Preferences};

impl Preferences {
    /// Sets the plugin `plugin`'s setting `key`, or shows why not.
    pub fn set_plugin_setting(
        &mut self,
        plugin: &str,
        key: &str,
        value: &str,
        cx: &mut Context<Self>,
    ) {
        let field = (plugin.to_owned(), key.to_owned());
        match crate::extensions::plugins::set_setting(plugin, key, value, cx) {
            Ok(()) => {
                self.plugin_errors.remove(&field);
            }
            Err(error) => {
                self.plugin_errors.insert(field, format!("{error:#}"));
            }
        }
        cx.notify();
    }

    /// Makes the fields of the plugins' text and number settings that are not there yet.
    pub(super) fn make_plugin_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for (plugin, spec, value) in plugin_settings(cx) {
            if spec.kind == birchpad_plugin::SettingKind::Bool {
                continue;
            }
            let field = (plugin, spec.key);
            if self.plugin_inputs.contains_key(&field) {
                continue;
            }
            let input = cx.new(|cx| InputState::new(window, cx).default_value(value));
            let (plugin, key) = field.clone();
            cx.subscribe(&input, move |this, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = input.read(cx).value().to_string();
                    this.set_plugin_setting(&plugin, &key, value.trim(), cx);
                }
            })
            .detach();
            self.plugin_inputs.insert(field, input);
        }
    }

    /// Preferences > Plugins: each plugin's name, then its settings.
    pub(super) fn render_plugin_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = ui();
        let mut rows: Vec<AnyElement> = Vec::new();
        let mut last_plugin: Option<String> = None;
        for (index, (plugin, spec, value)) in plugin_settings(cx).into_iter().enumerate() {
            if last_plugin.as_ref() != Some(&plugin) {
                rows.push(
                    div()
                        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                        .child(crate::extensions::plugins::plugin_name(&plugin, cx))
                        .into_any_element(),
                );
                last_plugin = Some(plugin.clone());
            }
            let selector = format!("plugin-setting-{plugin}-{}", spec.key);
            let field = (plugin, spec.key);
            let control = match spec.kind {
                birchpad_plugin::SettingKind::Bool => {
                    let checked = value == "true";
                    let (plugin, key) = field.clone();
                    Checkbox::new(("plugin-setting", index))
                        .debug_selector(move || selector.clone())
                        .label(spec.title)
                        .checked(checked)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let value = if checked { "false" } else { "true" };
                            this.set_plugin_setting(&plugin, &key, value, cx);
                        }))
                        .into_any_element()
                }
                birchpad_plugin::SettingKind::Number | birchpad_plugin::SettingKind::Text => div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(div().w(px(200.)).child(spec.title))
                    .child(
                        div()
                            .w(px(200.))
                            .debug_selector(move || selector.clone())
                            .children(self.plugin_inputs.get(&field).map(Input::new)),
                    )
                    .into_any_element(),
            };
            rows.push(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(control)
                    .children(self.plugin_errors.get(&field).map(|error| {
                        div()
                            .text_sm()
                            .text_color(paint(colors.error))
                            .child(error.clone())
                    }))
                    .into_any_element(),
            );
        }
        div()
            .debug_selector(|| format!("preferences-{PLUGIN_SETTINGS}"))
            .flex()
            .flex_col()
            .gap_2()
            .when(rows.is_empty(), |list| {
                list.child(
                    div()
                        .text_color(paint(colors.muted))
                        .child(t!("preferences.no-plugin-has-settings")),
                )
            })
            .children(rows)
            .into_any_element()
    }
}

/// The settings the installed plugins declare, in the order of the plugins and of their
/// manifests: the plugin's id, the setting, and its value now.
pub fn plugin_settings(cx: &App) -> Vec<(String, birchpad_plugin::SettingSpec, String)> {
    let Some(plugins) = cx.try_global::<crate::extensions::plugins::Plugins>() else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in &plugins.entries {
        if entry.installed.manifest.settings.is_empty() {
            continue;
        }
        let grants = crate::extensions::plugins::grants(entry.id(), cx);
        for spec in &entry.installed.manifest.settings {
            let value = grants.settings.get(&spec.key).cloned().unwrap_or_default();
            found.push((entry.id().to_owned(), spec.clone(), value));
        }
    }
    found
}
