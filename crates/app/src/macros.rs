//! The Macro menu, as in Notepad++ (ADR 0030): recording the commands run and the text typed,
//! playing them back once, several times or until the end of the file, and saving them by
//! name in `macros.toml` next to the settings.

use anyhow::{Context as _, Result, anyhow, bail};
use birchpad_commands::{Invocation, Macro};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::radio::RadioGroup;
use gpui_kit::{
    App, AppContext as _, Context, Entity, Global, WeakEntity, Window, div, prelude::*, px,
};
use serde::Deserialize;

use crate::app_state::AppState;
use crate::commands::{CommandRegistry, Register as _};
use crate::editor::widgets::error_text;
use crate::theme::{paint, ui};
use crate::workspace::{Workspace, report_error};

/// The most times "until the end of the file" runs a macro, in case it never gets there.
pub const UNTIL_END_LIMIT: usize = 100_000;

/// Macros of this run and the saved ones.
#[derive(Default)]
pub struct Macros {
    /// The steps recorded so far, while recording.
    recording: Option<Vec<Invocation>>,
    /// The last macro recorded, which Playback runs.
    pub current: Vec<Invocation>,
    /// While a macro plays, what it runs is not recorded.
    pub playing: bool,
    /// While a recorded step does more than one thing, the rest is not recorded.
    paused: bool,
    pub saved: Vec<Macro>,
}

impl Global for Macros {}

impl Macros {
    pub fn is_recording(&self) -> bool {
        self.recording.is_some()
    }
}

fn macros_path(cx: &App) -> Option<std::path::PathBuf> {
    AppState::global(cx)
        .paths
        .user_config_dir()
        .map(|dir| dir.join("macros.toml"))
}

/// Reads the saved macros.
pub fn init(cx: &mut App) {
    let mut macros = Macros::default();
    if let Some(path) = macros_path(cx)
        && let Ok(text) = std::fs::read_to_string(&path)
    {
        match birchpad_commands::parse_macros(&text) {
            Ok(saved) => macros.saved = saved,
            Err(error) => eprintln!("{}: {error}", path.display()),
        }
    }
    cx.set_global(macros);
}

fn macros(cx: &mut App) -> &mut Macros {
    if !cx.has_global::<Macros>() {
        cx.set_global(Macros::default());
    }
    cx.global_mut::<Macros>()
}

/// Notes a command that ran, or text typed, if a macro is being recorded.
pub fn record(invocation: &Invocation, cx: &mut App) {
    if !cx.has_global::<Macros>() || !birchpad_commands::is_recordable(&invocation.command) {
        return;
    }
    let macros = cx.global_mut::<Macros>();
    if macros.playing || macros.paused {
        return;
    }
    if let Some(steps) = &mut macros.recording {
        birchpad_commands::record(steps, invocation.clone());
    }
}

/// Notes `invocation` even if its command is not recorded when it runs (searches, recorded with
/// their text and options).
pub fn record_always(invocation: &Invocation, cx: &mut App) {
    if !cx.has_global::<Macros>() {
        return;
    }
    let macros = cx.global_mut::<Macros>();
    if macros.playing || macros.paused {
        return;
    }
    if let Some(steps) = &mut macros.recording {
        birchpad_commands::record(steps, invocation.clone());
    }
}

/// Stops (or starts again) noting what runs, while a recorded step does several things.
pub fn pause(paused: bool, cx: &mut App) {
    if cx.has_global::<Macros>() {
        cx.global_mut::<Macros>().paused = paused;
    }
}

/// Text typed into an editor, for a macro being recorded.
pub fn record_typing(text: &str, cx: &mut App) {
    if !text.is_empty() {
        record(
            &Invocation::with_args("edit.type", serde_json::json!({ "text": text })),
            cx,
        );
    }
}

/// Saves `found` as a macro named `name`, replacing one of that name.
pub fn save(name: &str, steps: Vec<Invocation>, cx: &mut App) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        bail!("{}", t!("macros.a-macro-needs-a-name"));
    }
    if steps.is_empty() {
        bail!("{}", t!("macros.there-is-no-recorded-macro-to-save"));
    }
    let path = macros_path(cx).ok_or_else(|| {
        anyhow!(
            "{}",
            t!("macros.there-is-no-settings-folder-for-macros-toml")
        )
    })?;
    let mut saved = macros(cx).saved.clone();
    let found = Macro {
        name: name.to_owned(),
        steps,
    };
    match saved.iter_mut().find(|known| known.name == name) {
        Some(known) => *known = found,
        None => saved.push(found),
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }
    std::fs::write(&path, birchpad_commands::write_macros(&saved))
        .with_context(|| format!("cannot write {}", path.display()))?;
    macros(cx).saved = saved;
    Ok(())
}

/// Deletes the saved macro `name` (Shortcut Mapper > Macros > Delete).
pub fn delete(name: &str, cx: &mut App) -> Result<()> {
    let path = macros_path(cx).ok_or_else(|| {
        anyhow!(
            "{}",
            t!("macros.there-is-no-settings-folder-for-macros-toml")
        )
    })?;
    let mut saved = macros(cx).saved.clone();
    let before = saved.len();
    saved.retain(|known| known.name != name);
    if saved.len() == before {
        bail!("{}", t!("macros.there-is-no-macro-named", name = name));
    }
    std::fs::write(&path, birchpad_commands::write_macros(&saved))
        .with_context(|| format!("cannot write {}", path.display()))?;
    macros(cx).saved = saved;
    Ok(())
}

/// Where the caret and the text stand, to tell whether a run of a macro got anywhere.
fn progress(workspace: &Workspace, cx: &App) -> Option<(usize, usize)> {
    let view = workspace.active_view(cx)?;
    let view = view.read(cx);
    let len = view.buffer.read(cx).doc().text().len();
    Some((view.selection.primary().head, len))
}

/// Runs `steps` `times` times, or until the caret reaches the end of the document (or stops
/// moving) with `until_end`. The first step that fails stops it.
pub fn play(
    workspace: &mut Workspace,
    steps: &[Invocation],
    times: usize,
    until_end: bool,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Result<()> {
    if steps.is_empty() {
        bail!("{}", t!("macros.there-is-no-macro-to-play"));
    }
    macros(cx).playing = true;
    let result = (|| {
        let runs = if until_end { UNTIL_END_LIMIT } else { times };
        for _ in 0..runs {
            if until_end
                && let Some((head, len)) = progress(workspace, cx)
                && head >= len
            {
                break;
            }
            let before = progress(workspace, cx);
            for step in steps {
                workspace
                    .dispatch(step, window, cx)
                    .with_context(|| format!("the macro stopped at {}", step.command))?;
            }
            if until_end && progress(workspace, cx) == before {
                break;
            }
        }
        Ok(())
    })();
    macros(cx).playing = false;
    result
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct RunArgs {
    /// A saved macro; unset: the one recorded last.
    #[serde(default)]
    name: Option<String>,
    #[serde(default = "one")]
    times: usize,
    #[serde(default)]
    until_end: bool,
}

fn one() -> usize {
    1
}

/// The steps of the saved macro `name`, or of the last recorded one.
fn steps_of(name: Option<&str>, cx: &mut App) -> Result<Vec<Invocation>> {
    let macros = macros(cx);
    match name {
        None => Ok(macros.current.clone()),
        Some(name) => macros
            .saved
            .iter()
            .find(|known| known.name == name)
            .map(|known| known.steps.clone())
            .ok_or_else(|| anyhow!("{}", t!("macros.there-is-no-macro-named", name = name))),
    }
}

pub fn register_commands(registry: &mut CommandRegistry) {
    // Notepad++'s built-in macro: Trim Trailing Space, then Save.
    registry.workspace("macro.trim-and-save", |this, (), window, cx| {
        let trim = Invocation::with_args("edit.trim", serde_json::json!({ "which": "trailing" }));
        this.dispatch(&trim, window, cx)?;
        this.dispatch(&Invocation::new("file.save"), window, cx)
    });
    // Ctrl+Shift+R starts recording and, pressed again, stops it, as in Notepad++.
    registry.workspace("macro.start-recording", |this, (), _, cx| {
        let macros = macros(cx);
        match macros.recording.take() {
            Some(steps) => macros.current = steps,
            None => macros.recording = Some(Vec::new()),
        }
        this.refresh_menus(cx);
        Ok(())
    });
    registry.workspace("macro.stop-recording", |this, (), _, cx| {
        let macros = macros(cx);
        if let Some(steps) = macros.recording.take() {
            macros.current = steps;
        }
        this.refresh_menus(cx);
        Ok(())
    });
    registry.enabled_when("macro.stop-recording", |cx| {
        cx.try_global::<Macros>().is_some_and(Macros::is_recording)
    });
    registry.workspace("macro.playback", |this, (), window, cx| {
        let steps = macros(cx).current.clone();
        play(this, &steps, 1, false, window, cx)
    });
    let can_play = |cx: &App| {
        cx.try_global::<Macros>()
            .is_some_and(|macros| !macros.is_recording() && !macros.current.is_empty())
    };
    registry.enabled_when("macro.playback", can_play);
    registry.workspace("macro.run", |this, args: RunArgs, window, cx| {
        if macros(cx).is_recording() {
            bail!(
                "{}",
                t!("macros.a-macro-cannot-run-while-one-is-being-recorded")
            );
        }
        let steps = steps_of(args.name.as_deref(), cx)?;
        play(this, &steps, args.times, args.until_end, window, cx)
    });
    registry.workspace("macro.save", |_, (), window, cx| {
        open_save(cx.entity().downgrade(), window, cx);
        Ok(())
    });
    registry.enabled_when("macro.save", can_play);
    registry.workspace("macro.run-multiple", |_, (), window, cx| {
        open_run_multiple(cx.entity().downgrade(), window, cx);
        Ok(())
    });
    registry.enabled_when("macro.run-multiple", |cx| {
        cx.try_global::<Macros>().is_some_and(|macros| {
            !macros.is_recording() && (!macros.current.is_empty() || !macros.saved.is_empty())
        })
    });
}

/// Save Current Recorded Macro...: asks for a name.
fn open_save(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut App) {
    let name = cx.new(|cx| InputState::new(window, cx).placeholder(t!("macros.name")));
    let error = cx.new(|_| None::<String>);
    let focus = name.clone();
    window.open_dialog(cx, move |dialog, _, _| {
        let (name, error, workspace) = (name.clone(), error.clone(), workspace.clone());
        let shown = error.clone();
        dialog
            .title(t!("macros.save-macro"))
            .w(px(360.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(Input::new(&name).id("macro-name"))
                    .child(MessageView(shown)),
            )
            .footer(
                DialogFooter::new()
                    .child(crate::workspace::dialog_close("common.cancel"))
                    .child(crate::workspace::dialog_action(
                        Button::new("macro-save").label(t!("common.save")),
                    )),
            )
            .on_ok(move |_, _, cx| {
                let text = name.read(cx).value().to_string();
                let steps = macros(cx).current.clone();
                match save(&text, steps, cx) {
                    Ok(()) => {
                        workspace
                            .update(cx, |workspace, cx| workspace.refresh_menus(cx))
                            .ok();
                        true
                    }
                    Err(problem) => {
                        error.update(cx, |error, cx| {
                            *error = Some(format!("{problem:#}"));
                            cx.notify();
                        });
                        false
                    }
                }
            })
    });
    focus.update(cx, |input, cx| input.focus(window, cx));
}

/// An error line under a dialog's fields.
#[derive(IntoElement)]
struct MessageView(Entity<Option<String>>);

impl RenderOnce for MessageView {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .text_color(paint(ui().error))
            .children(self.0.read(cx).clone())
    }
}

/// Run a Macro Multiple Times...: which macro, and how many times or until the end of the file.
pub struct RunMultiple {
    workspace: WeakEntity<Workspace>,
    /// "Current recorded macro" (when there is one), then the saved ones.
    choices: Vec<Option<String>>,
    pub choice: usize,
    pub until_end: bool,
    pub times: Entity<InputState>,
    pub error: Option<String>,
}

fn open_run_multiple(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut App) {
    let dialog = cx.new(|cx| RunMultiple::new(workspace, window, cx));
    window.open_dialog(cx, move |modal, _, _| {
        let run = dialog.clone();
        modal
            .title(t!("macros.run-a-macro-multiple-times"))
            .w(px(420.))
            .child(dialog.clone())
            .footer(
                DialogFooter::new()
                    .child(crate::workspace::dialog_close("common.cancel"))
                    .child(crate::workspace::dialog_action(
                        Button::new("macro-run").label(t!("common.run")),
                    )),
            )
            .on_ok(move |_, window, cx| run.update(cx, |this, cx| this.run(window, cx)))
    });
}

impl RunMultiple {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let macros = macros(cx);
        let mut choices = Vec::new();
        if !macros.current.is_empty() {
            choices.push(None);
        }
        choices.extend(macros.saved.iter().map(|known| Some(known.name.clone())));
        Self {
            workspace,
            choices,
            choice: 0,
            until_end: false,
            times: cx.new(|cx| InputState::new(window, cx).default_value("1")),
            error: None,
        }
    }

    /// Runs the chosen macro; false keeps the dialog open to show what went wrong.
    pub fn run(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let result = (|| {
            let name = self
                .choices
                .get(self.choice)
                .cloned()
                .ok_or_else(|| anyhow!("{}", t!("macros.there-is-no-macro-to-run")))?;
            let times = if self.until_end {
                1
            } else {
                let text = self.times.read(cx).value().to_string();
                match text.trim().parse::<usize>() {
                    Ok(times) if times > 0 => times,
                    _ => bail!(
                        "{}",
                        t!("macros.run-is-not-a-number-of-times", text = text.trim())
                    ),
                }
            };
            let steps = steps_of(name.as_deref(), cx)?;
            let until_end = self.until_end;
            self.workspace
                .update(cx, |workspace, cx| {
                    play(workspace, &steps, times, until_end, window, cx)
                })
                .map_err(|_| anyhow!("{}", t!("macros.the-window-is-closed")))?
        })();
        match result {
            Ok(()) => true,
            Err(error) => {
                // Playing errors are shown where they happened; this dialog says what it refused.
                self.error = Some(format!("{error:#}"));
                report_error(&error, window, cx);
                cx.notify();
                false
            }
        }
    }
}

impl Render for RunMultiple {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let labels: Vec<String> = self
            .choices
            .iter()
            .map(|choice| {
                choice
                    .clone()
                    .unwrap_or_else(|| t!("macros.current-recorded-macro"))
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                RadioGroup::vertical("macro-choice")
                    .children(labels)
                    .selected_index(Some(self.choice))
                    .on_click(cx.listener(|this, index: &usize, _, cx| {
                        this.choice = *index;
                        cx.notify();
                    })),
            )
            .child(
                RadioGroup::vertical("macro-times")
                    .children([t!("macros.run-times"), t!("macros.run-until-end")])
                    .selected_index(Some(usize::from(self.until_end)))
                    .on_click(cx.listener(|this, index: &usize, _, cx| {
                        this.until_end = *index == 1;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(t!("macros.times"))
                    .child(
                        div()
                            .w(px(100.))
                            .child(Input::new(&self.times).id("macro-times-input")),
                    ),
            )
            .children(self.error.clone().map(|error| error_text(error)))
    }
}
