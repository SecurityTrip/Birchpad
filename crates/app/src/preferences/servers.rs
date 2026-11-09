//! Preferences > Language Servers (ADR 0065): the server of each language, by its command line.

use anyhow::{Result, bail};
use gpui_kit::component::button::Button;
use gpui_kit::component::input::Input;
use gpui_kit::component::{Disableable as _, Sizable as _};
use gpui_kit::{AnyElement, Context, div, prelude::*, px};
use toml::Value;

use crate::app_state::AppState;

use super::values::set;
use super::{Preferences, label, language_id};

/// The key the table's problems are shown under.
pub(super) const SERVERS: &str = "lsp.servers";

impl Preferences {
    /// Set: the language typed gets the command line typed as its server.
    pub fn set_language_server(&mut self, cx: &mut Context<Self>) {
        let typed = self.server_language.read(cx).value().to_string();
        let command = self.server_command.read(cx).value().trim().to_owned();
        let result = server_language(&typed).and_then(|id| {
            birchpad_lsp::CommandLine::parse(&command)?;
            set(&format!("{SERVERS}.{id}"), Some(Value::String(command)), cx)
        });
        self.after_server_change(result, cx);
    }

    /// Remove: the language has no server again.
    pub fn remove_language_server(&mut self, id: &str, cx: &mut Context<Self>) {
        let result = set(&format!("{SERVERS}.{id}"), None, cx);
        self.after_server_change(result, cx);
    }

    fn after_server_change(&mut self, result: Result<()>, cx: &mut Context<Self>) {
        match result {
            Ok(()) => {
                self.errors.remove(SERVERS);
                // The servers follow: started, restarted with their new command, or stopped.
                self.workspace
                    .update(cx, |workspace, cx| workspace.refresh_views(cx))
                    .ok();
            }
            Err(error) => {
                self.errors.insert(SERVERS, format!("{error:#}"));
            }
        }
        cx.notify();
    }

    /// The table: one row per language with a server, then the fields that set one.
    pub(super) fn render_language_servers(
        &self,
        locked: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let servers = AppState::global(cx).settings.lsp.servers.clone();
        let rows = servers
            .iter()
            .filter(|(_, command)| !command.trim().is_empty())
            .enumerate()
            .map(|(index, (id, command))| {
                let id = id.clone();
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .w(px(360.))
                            .debug_selector(move || format!("server-row-{index}"))
                            .child(format!("{id}: {command}")),
                    )
                    .child(
                        Button::new(("server-remove", index))
                            .debug_selector(move || format!("server-remove-{index}"))
                            .small()
                            .label(t!("common.remove"))
                            .disabled(locked)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.remove_language_server(&id, cx);
                            })),
                    )
            });
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(label("Servers by language"))
            .children(rows)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .w(px(140.))
                            .debug_selector(|| "server-language".into())
                            .child(Input::new(&self.server_language)),
                    )
                    .child(
                        div()
                            .w(px(260.))
                            .debug_selector(|| "server-command".into())
                            .child(Input::new(&self.server_command)),
                    )
                    .child(
                        Button::new("server-set")
                            .debug_selector(|| "server-set".into())
                            .small()
                            .label(t!("preferences.set"))
                            .disabled(locked)
                            .on_click(cx.listener(|this, _, _, cx| this.set_language_server(cx))),
                    ),
            )
            .into_any_element()
    }
}

/// The language of a server as typed: a language with a grammar (by its id or name); plain
/// text has none.
pub fn server_language(typed: &str) -> Result<String> {
    let id = language_id(typed)?;
    if id == "text" {
        bail!("{}", t!("preferences.text-has-no-server"));
    }
    Ok(id)
}
