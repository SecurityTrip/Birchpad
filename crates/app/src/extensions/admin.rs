//! Plugins > Plugins Admin... (ADR 0038): the installed plugins with their version, state and
//! description; installing one from its `plugin.toml`, removing one, turning one on or off.
//! Each change reloads the plugins.
//!
//! The dialog's buttons run commands (`plugin.install`, `plugin.uninstall`, `plugin.enable`),
//! so they can be bound to keys and used by scripts too.

use std::path::PathBuf;

use anyhow::{Result, anyhow};
use birchpad_config::PluginInstall;
use birchpad_plugin::manager;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::{
    App, AppContext as _, Context, FontWeight, WeakEntity, Window, div, prelude::*, px,
};
use serde::Deserialize;

use super::plugins::{Plugins, State, plugins_dir};
use crate::app_state::AppState;
use crate::commands::{CommandRegistry, Register as _};
use crate::workspace::{Workspace, report_error};

#[derive(Deserialize)]
struct InstallArgs {
    /// The plugin's `plugin.toml`; unset asks for it.
    #[serde(default)]
    path: Option<PathBuf>,
}

#[derive(Deserialize)]
struct PluginArgs {
    plugin: String,
}

#[derive(Deserialize)]
struct EnableArgs {
    plugin: String,
    enabled: bool,
}

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("plugin.admin", |_, (), window, cx| {
        open(window, cx);
        Ok(())
    });
    registry.workspace(
        "plugin.install",
        |this, args: InstallArgs, window, cx| match args.path {
            Some(path) => this.install_plugin(&path, window, cx),
            None => {
                this.ask_plugin_to_install(window, cx);
                Ok(())
            }
        },
    );
    registry.workspace("plugin.uninstall", |this, args: PluginArgs, window, cx| {
        let dir = plugins_dir(cx)
            .ok_or_else(|| anyhow!("{}", t!("extensions.admin.there-is-no-settings-folder")))?;
        let result = manager::uninstall(&dir, &args.plugin);
        this.load_plugins(window, cx);
        result
    });
    registry.workspace("plugin.enable", |this, args: EnableArgs, window, cx| {
        let dir = plugins_dir(cx)
            .ok_or_else(|| anyhow!("{}", t!("extensions.admin.there-is-no-settings-folder")))?;
        let known = cx.try_global::<Plugins>().is_some_and(|plugins| {
            plugins
                .entries
                .iter()
                .any(|entry| entry.id() == args.plugin)
        });
        if !known {
            anyhow::bail!(
                "{}",
                t!(
                    "extensions.admin.the-plugin-is-not-installed",
                    plugin = args.plugin
                )
            );
        }
        manager::set_enabled(&dir, &args.plugin, args.enabled)?;
        this.load_plugins(window, cx);
        Ok(())
    });
}

impl Workspace {
    fn install_plugin(
        &mut self,
        manifest: &std::path::Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let dir = plugins_dir(cx)
            .ok_or_else(|| anyhow!("{}", t!("extensions.admin.there-is-no-settings-folder")))?;
        let settings = AppState::global(cx).settings.plugins.clone();
        let installed = manager::install(manifest, &dir, &settings)?;
        self.load_plugins(window, cx);
        crate::workspace::report_success(
            format!(
                "{} {}",
                t!("extensions.admin.installed"),
                installed.manifest.name
            ),
            window,
            cx,
        );
        Ok(())
    }

    /// Install...: asks for a plugin's `plugin.toml`.
    fn ask_plugin_to_install(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let start = self.default_directory(cx);
        cx.spawn_in(window, async move |this, cx| {
            let Some(paths) = crate::path_dialog::ask_open(start, cx).await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                if let Err(error) = this.install_plugin(&path, window, cx) {
                    report_error(&error, window, cx);
                }
            })
            .ok();
        })
        .detach();
    }
}

/// A row of the dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) description: String,
    pub(crate) status: String,
    /// Whether the plugin is turned off (or would be, were it allowed).
    pub(crate) disabled: bool,
}

/// What the dialog lists: the plugins, and the folders that hold none that reads.
pub(crate) fn rows(cx: &App) -> (Vec<Row>, Vec<String>) {
    let Some(plugins) = cx.try_global::<Plugins>() else {
        return (Vec::new(), Vec::new());
    };
    let rows = plugins
        .entries
        .iter()
        .map(|entry| {
            let manifest = &entry.installed.manifest;
            let status = match &entry.state {
                State::Loading => t!("extensions.admin.loading"),
                State::Ready(_) => t!("extensions.admin.on"),
                State::Disabled => t!("extensions.admin.off"),
                State::Refused(why) | State::Failed(why) => why.clone(),
            };
            Row {
                id: manifest.id.clone(),
                name: manifest.name.clone(),
                version: manifest.version.clone(),
                description: manifest.description.clone(),
                status,
                disabled: matches!(entry.state, State::Disabled),
            }
        })
        .collect();
    (rows, plugins.problems.clone())
}

/// What the administrator's policy says, if it limits plugins.
fn policy_note(cx: &App) -> Option<String> {
    match AppState::global(cx).settings.plugins.install {
        PluginInstall::Allow => None,
        PluginInstall::Allowlist => Some(t!(
            "extensions.admin.your-administrator-allows-only-some-plugins"
        )),
        PluginInstall::Deny => Some(t!(
            "extensions.admin.your-administrator-has-turned-plugins-off"
        )),
    }
}

/// The dialog's content.
pub(crate) struct PluginsAdmin {
    workspace: WeakEntity<Workspace>,
}

fn open(window: &mut Window, cx: &mut Context<Workspace>) {
    let handle = cx.entity().downgrade();
    let install_from = handle.clone();
    let admin = cx.new(|_| PluginsAdmin { workspace: handle });
    window.open_dialog(cx, move |dialog, _, _| {
        let install_from = install_from.clone();
        dialog
            .title(t!("extensions.admin.plugins-admin"))
            .w(px(640.))
            .child(admin.clone())
            .footer(
                DialogFooter::new()
                    .child(
                        Button::new("plugin-install")
                            .label(t!("extensions.admin.install"))
                            .on_click(move |_, window, cx| {
                                install_from
                                    .update(cx, |workspace, cx| {
                                        workspace.ask_plugin_to_install(window, cx);
                                    })
                                    .ok();
                            }),
                    )
                    .child(crate::workspace::dialog_close("common.close")),
            )
    });
}

impl PluginsAdmin {
    fn run(&self, invocation: birchpad_commands::Invocation, window: &mut Window, cx: &mut App) {
        self.workspace
            .update(cx, |workspace, cx| {
                if let Err(error) = workspace.dispatch(&invocation, window, cx) {
                    report_error(&error, window, cx);
                }
            })
            .ok();
    }
}

impl Render for PluginsAdmin {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (rows, problems) = rows(cx);
        let muted = crate::theme::paint(crate::theme::ui().muted);
        let error = crate::theme::paint(crate::theme::ui().error);
        let border = crate::theme::paint(crate::theme::ui().border);
        let empty = rows.is_empty();
        let rows = rows.into_iter().enumerate().map(|(index, row)| {
            let toggle = birchpad_commands::Invocation::with_args(
                "plugin.enable",
                serde_json::json!({ "plugin": row.id, "enabled": row.disabled }),
            );
            let remove = birchpad_commands::Invocation::with_args(
                "plugin.uninstall",
                serde_json::json!({ "plugin": row.id }),
            );
            let toggle_label = if row.disabled {
                "extensions.admin.turn-on"
            } else {
                "extensions.admin.turn-off"
            };
            div()
                .id(("plugin-row", index))
                .debug_selector(move || format!("plugin-row-{index}"))
                .flex()
                .flex_row()
                .gap_2()
                .py_1()
                .border_b_1()
                .border_color(border)
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .gap_2()
                                .child(div().font_weight(FontWeight::SEMIBOLD).child(row.name))
                                .child(div().text_color(muted).child(row.version)),
                        )
                        .child(div().text_color(muted).child(row.description))
                        .child(div().child(row.status)),
                )
                .child(
                    Button::new(("plugin-toggle", index))
                        .small()
                        .label(t!(toggle_label))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.run(toggle.clone(), window, cx);
                            cx.notify();
                        })),
                )
                .child(
                    Button::new(("plugin-remove", index))
                        .small()
                        .ghost()
                        .label(t!("common.remove"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.run(remove.clone(), window, cx);
                            cx.notify();
                        })),
                )
        });
        div()
            .flex()
            .flex_col()
            .gap_1()
            .max_h(px(420.))
            .when_some(policy_note(cx), |list, note| {
                list.child(div().text_color(error).child(note))
            })
            .when(empty, |list| {
                list.child(
                    div()
                        .text_color(muted)
                        .child(t!("extensions.admin.no-plugins-are-installed")),
                )
            })
            .child(
                div()
                    .id("plugin-rows")
                    .flex()
                    .flex_col()
                    .overflow_y_scroll()
                    .children(rows),
            )
            .children(
                problems
                    .into_iter()
                    .map(move |problem| div().text_color(error).child(problem)),
            )
    }
}

#[cfg(test)]
mod tests;
