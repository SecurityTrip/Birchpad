//! WebAssembly plugins in the window (ADR 0036, 0038): loading the plugins of the `plugins`
//! folder that are turned on and that the administrator allows, their commands in the Plugins
//! menu, their events, and clicks on their panel.
//!
//! Components are compiled off the main thread when Birchpad starts (and on Reload Plugins);
//! their commands appear in the menu once they are ready.

use std::path::PathBuf;

use anyhow::{Context as _, Result, anyhow, bail};
use birchpad_plugin::manager::{self, Installed};
use birchpad_plugin::{EventKind, Plugin, PluginCommand, Runtime};
use gpui_kit::{App, AppContext as _, Context, Entity, Global, Window};
use serde::Deserialize;

use super::PanelOwner;
use crate::app_state::AppState;
use crate::buffer::Buffer;
use crate::commands::{CommandRegistry, Register as _};
use crate::editor::EditorView;
use crate::workspace::Workspace;

/// Where a plugin stands.
pub enum State {
    /// Being compiled.
    Loading,
    Ready(Box<Plugin>),
    /// Turned off in Plugins Admin.
    Disabled,
    /// Not allowed by the administrator's policy, or broken: why.
    Refused(String),
    Failed(String),
}

pub struct Entry {
    pub installed: Installed,
    pub state: State,
}

impl Entry {
    pub fn id(&self) -> &str {
        &self.installed.manifest.id
    }

    pub fn commands(&self) -> &[PluginCommand] {
        match &self.state {
            State::Ready(plugin) => plugin.commands(),
            _ => &[],
        }
    }
}

/// The installed plugins, and folders that hold none that reads.
#[derive(Default)]
pub struct Plugins {
    runtime: Option<Runtime>,
    pub entries: Vec<Entry>,
    pub problems: Vec<String>,
    /// Bumped by every load, so that a slow compilation of an earlier one is dropped.
    generation: u64,
    /// The view plugins last heard was activated: it is not activated again until another is.
    last_activated: Option<gpui_kit::EntityId>,
}

impl Global for Plugins {}

pub fn plugins_dir(cx: &App) -> Option<PathBuf> {
    AppState::global(cx)
        .paths
        .user_config_dir()
        .map(|dir| dir.join("plugins"))
}

fn plugins(cx: &mut App) -> &mut Plugins {
    if !cx.has_global::<Plugins>() {
        cx.set_global(Plugins::default());
    }
    cx.global_mut::<Plugins>()
}

/// The plugins' menus: each ready plugin's name with its commands.
pub fn menus(cx: &App) -> Vec<(String, String, Vec<PluginCommand>)> {
    cx.try_global::<Plugins>()
        .map(|plugins| {
            plugins
                .entries
                .iter()
                .filter(|entry| !entry.commands().is_empty())
                .map(|entry| {
                    (
                        entry.id().to_owned(),
                        entry.installed.manifest.name.clone(),
                        entry.commands().to_vec(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Deserialize)]
struct RunArgs {
    plugin: String,
    command: String,
}

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("plugin.run", |this, args: RunArgs, window, cx| {
        let (view, session) = super::session(this, cx);
        let outcome = super::nested(|| {
            with_plugin(&args.plugin, cx, |plugin| {
                plugin.run(&args.command, session)
            })
        })?;
        this.apply_outcome(
            view.as_ref(),
            PanelOwner::Plugin(args.plugin),
            outcome,
            window,
            cx,
        )
    });
    registry.workspace("plugin.reload", |this, (), window, cx| {
        this.load_plugins(window, cx);
        Ok(())
    });
    registry.workspace("plugin.open-folder", |_, (), _, cx| {
        let dir = plugins_dir(cx)
            .ok_or_else(|| anyhow!("{}", t!("extensions.plugins.there-is-no-settings-folder")))?;
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("cannot create {}", dir.display()))?;
        cx.open_with_system(&dir);
        Ok(())
    });
}

/// Calls the ready plugin `id`.
fn with_plugin<T>(
    id: &str,
    cx: &mut App,
    call: impl FnOnce(&mut Plugin) -> Result<T>,
) -> Result<T> {
    let entry = plugins(cx)
        .entries
        .iter_mut()
        .find(|entry| entry.id() == id)
        .ok_or_else(|| anyhow!("{}", t!("extensions.plugins.there-is-no-plugin", id = id)))?;
    match &mut entry.state {
        State::Ready(plugin) => call(plugin),
        State::Loading => bail!(
            "{}",
            t!("extensions.plugins.the-plugin-is-still-loading", id = id)
        ),
        State::Disabled => bail!(
            "{}",
            t!("extensions.plugins.the-plugin-is-turned-off", id = id)
        ),
        State::Refused(why) | State::Failed(why) => bail!(
            "{}",
            t!("extensions.plugins.the-plugin", id = id, why = why)
        ),
    }
}

impl Workspace {
    /// Reads the plugins folder and compiles the plugins that may run, in the background.
    pub fn load_plugins(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dir) = plugins_dir(cx) else {
            return;
        };
        let settings = AppState::global(cx).settings.plugins.clone();
        let found = manager::discover(&dir);
        let disabled = manager::read_disabled(&dir);
        let state = plugins(cx);
        state.generation += 1;
        let generation = state.generation;
        state.problems = found.problems;
        state.entries = found
            .plugins
            .into_iter()
            .map(|installed| {
                let id = &installed.manifest.id;
                let state = if let Err(why) = manager::permitted(id, &settings) {
                    State::Refused(why)
                } else if disabled.contains(id) {
                    State::Disabled
                } else {
                    State::Loading
                };
                Entry { installed, state }
            })
            .collect();
        let runtime = match &state.runtime {
            Some(runtime) => runtime.clone(),
            None => match Runtime::new() {
                Ok(runtime) => {
                    state.runtime = Some(runtime.clone());
                    runtime
                }
                Err(error) => {
                    let why = format!("{error:#}");
                    for entry in &mut state.entries {
                        if matches!(entry.state, State::Loading) {
                            entry.state = State::Failed(why.clone());
                        }
                    }
                    return;
                }
            },
        };
        let jobs: Vec<(String, PathBuf)> = state
            .entries
            .iter()
            .filter(|entry| matches!(entry.state, State::Loading))
            .map(|entry| (entry.id().to_owned(), entry.installed.entry()))
            .collect();
        self.refresh_menus(cx);
        for (id, path) in jobs {
            let compiler = runtime.clone();
            let compiled = cx.background_spawn(async move {
                let bytes = std::fs::read(&path)
                    .with_context(|| format!("cannot read {}", path.display()))?;
                compiler.compile(&bytes)
            });
            let runtime = runtime.clone();
            cx.spawn_in(window, async move |this, cx| {
                let compiled = compiled.await;
                this.update_in(cx, |this, _, cx| {
                    let loaded = compiled.and_then(|component| runtime.load(component));
                    let state = plugins(cx);
                    if state.generation != generation {
                        return;
                    }
                    if let Some(entry) = state.entries.iter_mut().find(|entry| entry.id() == id) {
                        entry.state = match loaded {
                            Ok(plugin) => State::Ready(Box::new(plugin)),
                            Err(error) => State::Failed(format!("{error:#}")),
                        };
                    }
                    this.refresh_menus(cx);
                })
                .ok();
            })
            .detach();
        }
    }

    /// A click on line `line` of a plugin's panel.
    pub fn plugin_panel_click(
        &mut self,
        id: &str,
        line: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let line = u32::try_from(line).unwrap_or(u32::MAX);
        let (view, session) = super::session(self, cx);
        let outcome =
            super::nested(|| with_plugin(id, cx, |plugin| plugin.panel_click(line, session)))?;
        self.apply_outcome(
            view.as_ref(),
            PanelOwner::Plugin(id.to_owned()),
            outcome,
            window,
            cx,
        )
    }

    /// Tells the plugins that asked for `event` about it, with `view`'s document. Their
    /// failures are reported, not returned: the event has happened anyway.
    pub fn plugin_event(
        &mut self,
        event: EventKind,
        view: &Entity<EditorView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event == EventKind::Activated {
            let id = Some(view.entity_id());
            let plugins = plugins(cx);
            if plugins.last_activated == id {
                return;
            }
            plugins.last_activated = id;
        }
        let ids: Vec<String> = cx
            .try_global::<Plugins>()
            .map(|plugins| {
                plugins
                    .entries
                    .iter()
                    .filter(|entry| {
                        matches!(entry.state, State::Ready(_))
                            && entry.installed.manifest.events.contains(&event)
                    })
                    .map(|entry| entry.id().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        for id in ids {
            let session = birchpad_plugin::Session::new(super::document_state(view.read(cx), cx));
            let result =
                super::nested(|| with_plugin(&id, cx, |plugin| plugin.event(event, session)))
                    .and_then(|outcome| {
                        self.apply_outcome(
                            Some(view),
                            PanelOwner::Plugin(id.clone()),
                            outcome,
                            window,
                            cx,
                        )
                    });
            if let Err(error) = result {
                crate::workspace::report_error(&error.context(format!("plugin {id}")), window, cx);
            }
        }
    }

    /// The view that shows `buffer`, for an event about it.
    pub fn view_of_buffer(&self, buffer: &Entity<Buffer>, cx: &App) -> Option<Entity<EditorView>> {
        self.all_views(cx)
            .into_iter()
            .find(|view| &view.read(cx).buffer == buffer)
    }
}
