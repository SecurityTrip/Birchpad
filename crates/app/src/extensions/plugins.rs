//! WebAssembly plugins in the window (ADR 0036, 0038, 0046): loading the plugins of the
//! `plugins` folder that are turned on and that the administrator allows, their commands in
//! the Plugins menu and the text's context menu, their settings and permissions, their events,
//! the command line's message for them, clicks on their panel, and their background tasks.
//!
//! Components are compiled off the main thread when Birchpad starts (and on Reload Plugins);
//! their commands appear in the menu once they are ready.

use std::path::PathBuf;

use anyhow::{Context as _, Result, anyhow, bail};
use birchpad_core::{RevisionId, Selection};
use birchpad_plugin::manager::{self, Installed};
use birchpad_plugin::{EventKind, Grants, Plugin, PluginCommand, Runtime, Session};
use gpui_kit::{App, AppContext as _, Context, Entity, EntityId, Global, Window};
use serde::Deserialize;

use super::PanelOwner;
use crate::app_state::AppState;
use crate::buffer::Buffer;
use crate::commands::{CommandRegistry, Register as _};
use crate::editor::EditorView;
use crate::workspace::Workspace;

/// The most background tasks of all plugins that run at once; more are refused.
pub const MAX_RUNNING_TASKS: usize = 8;

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

    /// Whether the plugin is ready and hears `event`.
    fn hears(&self, event: EventKind) -> bool {
        matches!(self.state, State::Ready(_)) && self.installed.manifest.events.contains(&event)
    }
}

/// The document and selection plugins last heard about, so that each change is told once.
#[derive(Debug, Clone, PartialEq)]
struct Seen {
    view: EntityId,
    revision: RevisionId,
    selection: Selection,
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
    last_activated: Option<EntityId>,
    /// The command line's `-pluginMessage`, for the plugins still loading when it came.
    pub message: Option<String>,
    /// What plugins last heard of the active document's text and selection.
    seen: Option<Seen>,
    /// Whether Changed or Selection Changed waits to be sent.
    change_pending: bool,
    /// The background tasks running.
    pub running_tasks: usize,
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

/// The commands plugins add to the text's context menu: the plugin's id and the command.
pub fn context_menu_commands(cx: &App) -> Vec<(String, PluginCommand)> {
    menus(cx)
        .into_iter()
        .flat_map(|(id, _, commands)| {
            commands
                .into_iter()
                .filter(|command| command.context_menu)
                .map(move |command| (id.clone(), command))
        })
        .collect()
}

/// The name of the installed plugin `id`, or its id.
pub fn plugin_name(id: &str, cx: &App) -> String {
    cx.try_global::<Plugins>()
        .and_then(|plugins| plugins.entries.iter().find(|entry| entry.id() == id))
        .map_or_else(
            || id.to_owned(),
            |entry| entry.installed.manifest.name.clone(),
        )
}

/// What the installed plugin `id` may use: its settings and, if allowed, files.
pub fn grants(id: &str, cx: &App) -> Grants {
    let (Some(dir), Some(plugins)) = (plugins_dir(cx), cx.try_global::<Plugins>()) else {
        return Grants::default();
    };
    plugins
        .entries
        .iter()
        .find(|entry| entry.id() == id)
        .map(|entry| manager::grants(&dir, &entry.installed))
        .unwrap_or_default()
}

/// Keeps `value` as the plugin `id`'s setting `key`. A setting its manifest declares takes
/// only the values of its kind.
pub fn set_setting(id: &str, key: &str, value: &str, cx: &App) -> Result<()> {
    let dir = plugins_dir(cx)
        .ok_or_else(|| anyhow!("{}", t!("extensions.plugins.there-is-no-settings-folder")))?;
    let entry = cx
        .try_global::<Plugins>()
        .and_then(|plugins| plugins.entries.iter().find(|entry| entry.id() == id))
        .ok_or_else(|| anyhow!("{}", t!("extensions.plugins.there-is-no-plugin", id = id)))?;
    if let Some(spec) = entry.installed.manifest.setting(key)
        && !spec.accepts(value)
    {
        bail!(
            "{}",
            t!(
                "extensions.plugins.not-a-value-of-the-setting",
                value = format!("{value:?}"),
                key = key
            )
        );
    }
    manager::write_setting(&dir, id, key, value)
}

#[derive(Deserialize)]
struct RunArgs {
    plugin: String,
    command: String,
}

#[derive(Deserialize)]
struct SettingArgs {
    plugin: String,
    key: String,
    value: String,
}

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("plugin.run", |this, args: RunArgs, window, cx| {
        let (view, session) = plugin_session(this, &args.plugin, cx);
        let outcome = super::nested(|| {
            with_plugin(&args.plugin, cx, |plugin| {
                plugin.run(&args.command, session)
            })
        })?;
        this.apply_plugin_outcome(view.as_ref(), &args.plugin, outcome, window, cx)
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
    registry.workspace("plugin.set-setting", |_, args: SettingArgs, _, cx| {
        set_setting(&args.plugin, &args.key, &args.value, cx)
    });
}

/// A session over the active document for the plugin `id`, with what it may use.
fn plugin_session(
    workspace: &Workspace,
    id: &str,
    cx: &App,
) -> (Option<Entity<EditorView>>, Session) {
    let (view, session) = super::session(workspace, cx);
    (view, session.with_grants(grants(id, cx)))
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
                this.update_in(cx, |this, window, cx| {
                    let loaded = compiled.and_then(|component| runtime.load(component));
                    let state = plugins(cx);
                    if state.generation != generation {
                        return;
                    }
                    let mut ready = false;
                    if let Some(entry) = state.entries.iter_mut().find(|entry| entry.id() == id) {
                        ready = loaded.is_ok();
                        entry.state = match loaded {
                            Ok(plugin) => State::Ready(Box::new(plugin)),
                            Err(error) => State::Failed(format!("{error:#}")),
                        };
                    }
                    this.refresh_menus(cx);
                    // A message of the command line that came while it loaded.
                    if ready && let Some(message) = plugins(cx).message.clone() {
                        this.send_plugin_message(&id, &message, window, cx);
                    }
                })
                .ok();
            })
            .detach();
        }
    }

    /// Applies what a call of the plugin `id` returned, then notes the document as plugins
    /// have seen it, so that the plugin's own edits are not told back to it as changes.
    fn apply_plugin_outcome(
        &mut self,
        view: Option<&Entity<EditorView>>,
        id: &str,
        outcome: birchpad_plugin::Outcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let result =
            self.apply_outcome(view, PanelOwner::Plugin(id.to_owned()), outcome, window, cx);
        self.note_seen_by_plugins(cx);
        result
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
        let (view, session) = plugin_session(self, id, cx);
        let outcome =
            super::nested(|| with_plugin(id, cx, |plugin| plugin.panel_click(line, session)))?;
        self.apply_plugin_outcome(view.as_ref(), id, outcome, window, cx)
    }

    /// The control `control` of the plugin `id`'s panel was used, with `value`.
    pub fn plugin_control(
        &mut self,
        id: &str,
        control: &str,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let (view, session) = plugin_session(self, id, cx);
        let outcome = super::nested(|| {
            with_plugin(id, cx, |plugin| plugin.control(control, value, session))
        })?;
        self.apply_plugin_outcome(view.as_ref(), id, outcome, window, cx)
    }

    /// Hands the command line's `-pluginMessage` to the plugins that ask for it: those ready
    /// now, and those still loading once they are.
    pub fn deliver_plugin_message(
        &mut self,
        message: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = plugins(cx);
        state.message = Some(message.to_owned());
        let ids: Vec<String> = state
            .entries
            .iter()
            .filter(|entry| entry.hears(EventKind::Message))
            .map(|entry| entry.id().to_owned())
            .collect();
        for id in ids {
            self.send_plugin_message(&id, message, window, cx);
        }
    }

    /// Hands `message` to the plugin `id`, if it asks for messages.
    fn send_plugin_message(
        &mut self,
        id: &str,
        message: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let hears = cx.try_global::<Plugins>().is_some_and(|plugins| {
            plugins
                .entries
                .iter()
                .any(|entry| entry.id() == id && entry.hears(EventKind::Message))
        });
        if !hears {
            return;
        }
        let (view, session) = plugin_session(self, id, cx);
        let result =
            super::nested(|| with_plugin(id, cx, |plugin| plugin.message(message, session)))
                .and_then(|outcome| {
                    self.apply_plugin_outcome(view.as_ref(), id, outcome, window, cx)
                });
        if let Err(error) = result {
            crate::workspace::report_error(&error.context(format!("plugin {id}")), window, cx);
        }
    }

    /// Runs the plugin `id`'s task `task` with `input` in the background; when it is done, the
    /// plugin hears what it returned through `on-task-done`.
    pub fn start_plugin_task(
        &mut self,
        id: &str,
        task: String,
        input: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let started = (|| {
            if cx
                .try_global::<Plugins>()
                .is_some_and(|plugins| plugins.running_tasks >= MAX_RUNNING_TASKS)
            {
                bail!(
                    "{}",
                    t!(
                        "extensions.plugins.too-many-tasks",
                        limit = MAX_RUNNING_TASKS
                    )
                );
            }
            with_plugin(id, cx, |plugin| Ok(plugin.task_runner()))
        })();
        let runner = match started {
            Ok(runner) => runner,
            Err(error) => {
                crate::workspace::report_error(&error.context(format!("plugin {id}")), window, cx);
                return;
            }
        };
        plugins(cx).running_tasks += 1;
        let session = Session::empty().with_grants(grants(id, cx));
        let work = {
            let task = task.clone();
            cx.background_spawn(async move { runner.run(&task, &input, session) })
        };
        let id = id.to_owned();
        cx.spawn_in(window, async move |this, cx| {
            let done = work.await;
            this.update_in(cx, |this, window, cx| {
                let state = plugins(cx);
                state.running_tasks = state.running_tasks.saturating_sub(1);
                let result = done.and_then(|(returned, outcome)| {
                    this.apply_plugin_outcome(None, &id, outcome, window, cx)?;
                    let (view, session) = plugin_session(this, &id, cx);
                    let outcome = super::nested(|| {
                        with_plugin(&id, cx, |plugin| {
                            plugin.task_done(&task, &returned, session)
                        })
                    })?;
                    this.apply_plugin_outcome(view.as_ref(), &id, outcome, window, cx)
                });
                if let Err(error) = result {
                    crate::workspace::report_error(
                        &error.context(format!("plugin {id}")),
                        window,
                        cx,
                    );
                }
            })
            .ok();
        })
        .detach();
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
                    .filter(|entry| entry.hears(event))
                    .map(|entry| entry.id().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        for id in ids {
            let session =
                Session::new(super::document_state(view.read(cx), cx)).with_grants(grants(&id, cx));
            let result =
                super::nested(|| with_plugin(&id, cx, |plugin| plugin.event(event, session)))
                    .and_then(|outcome| {
                        self.apply_plugin_outcome(Some(view), &id, outcome, window, cx)
                    });
            if let Err(error) = result {
                crate::workspace::report_error(&error.context(format!("plugin {id}")), window, cx);
            }
        }
    }

    /// What plugins would see of the active document now.
    fn seen_now(&self, cx: &App) -> Option<Seen> {
        let view = self.active_view(cx)?;
        let editor = view.read(cx);
        Some(Seen {
            view: view.entity_id(),
            revision: editor.buffer.read(cx).doc().revision(),
            selection: editor.selection.clone(),
        })
    }

    /// Notes the active document as plugins have seen it.
    pub fn note_seen_by_plugins(&self, cx: &mut App) {
        let seen = self.seen_now(cx);
        plugins(cx).seen = seen;
    }

    /// The active view changed somehow: if its text or its selection did, and a plugin hears
    /// about that, tells it once the current update is done (so typing a few characters in
    /// one go is one change).
    pub fn watch_for_plugins(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let listening = cx.try_global::<Plugins>().is_some_and(|plugins| {
            !plugins.change_pending
                && plugins.entries.iter().any(|entry| {
                    entry.hears(EventKind::Changed) || entry.hears(EventKind::SelectionChanged)
                })
        });
        if !listening {
            return;
        }
        plugins(cx).change_pending = true;
        cx.defer_in(window, |this, window, cx| {
            plugins(cx).change_pending = false;
            let Some(now) = this.seen_now(cx) else {
                return;
            };
            let before = plugins(cx).seen.replace(now.clone());
            let event = match before {
                // Another document: Activated tells about it.
                Some(before) if before.view != now.view => return,
                None => return,
                Some(before) if before.revision != now.revision => EventKind::Changed,
                Some(before) if before.selection != now.selection => EventKind::SelectionChanged,
                Some(_) => return,
            };
            if let Some(view) = this.active_view(cx) {
                this.plugin_event(event, &view, window, cx);
            }
        });
    }

    /// The view that shows `buffer`, for an event about it.
    pub fn view_of_buffer(&self, buffer: &Entity<Buffer>, cx: &App) -> Option<Entity<EditorView>> {
        self.all_views(cx)
            .into_iter()
            .find(|view| &view.read(cx).buffer == buffer)
    }
}
