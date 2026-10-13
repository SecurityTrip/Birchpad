//! The Run menu, as in Notepad++: Run... asks for a command line with variables
//! (`$(FULL_CURRENT_PATH)`, `$(CURRENT_WORD)`, ...) and runs it; command lines saved by name in
//! `run.toml` are listed below it. A line that starts with `http://` or `https://` opens in the
//! browser; any other runs through the system's shell, without waiting for it.

use anyhow::{Context as _, Result, anyhow, bail};
use birchpad_commands::{RunEntry, RunVariables, is_url, shell_command};
use birchpad_core::motion::{line_of, line_range, word_at};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::{
    App, AppContext as _, Context, Entity, Global, WeakEntity, Window, div, prelude::*, px,
};
use serde::Deserialize;

use crate::app_state::AppState;
use crate::commands::{CommandRegistry, Register as _};
use crate::theme::{paint, ui};
use crate::workspace::Workspace;

/// The saved command lines, and the one run last (which Run... offers again).
#[derive(Default)]
pub(crate) struct RunMenu {
    pub(crate) saved: Vec<RunEntry>,
    last: String,
}

impl Global for RunMenu {}

fn run_path(cx: &App) -> Option<std::path::PathBuf> {
    AppState::global(cx)
        .paths
        .user_config_dir()
        .map(|dir| dir.join("run.toml"))
}

/// Reads `run.toml`; Notepad++'s defaults when there is none.
pub(crate) fn init(cx: &mut App) {
    let saved = match run_path(cx).map(|path| (std::fs::read_to_string(&path), path)) {
        Some((Ok(text), path)) => {
            birchpad_commands::parse_run_commands(&text).unwrap_or_else(|error| {
                eprintln!("{}: {error}", path.display());
                Vec::new()
            })
        }
        _ => birchpad_commands::default_run_commands(),
    };
    cx.set_global(RunMenu {
        saved,
        last: String::new(),
    });
}

fn run_menu(cx: &mut App) -> &mut RunMenu {
    if !cx.has_global::<RunMenu>() {
        cx.set_global(RunMenu {
            saved: birchpad_commands::default_run_commands(),
            last: String::new(),
        });
    }
    cx.global_mut::<RunMenu>()
}

/// What the variables stand for in the active document.
pub(crate) fn variables(workspace: &Workspace, cx: &App) -> RunVariables {
    let executable = std::env::current_exe().unwrap_or_default();
    let mut vars = RunVariables {
        executable: executable.display().to_string(),
        executable_directory: executable
            .parent()
            .map(|dir| dir.display().to_string())
            .unwrap_or_default(),
        ..RunVariables::default()
    };
    let Some(view) = workspace.active_view(cx) else {
        return vars;
    };
    let view = view.read(cx);
    let buffer = view.buffer.read(cx);
    if let Some(path) = buffer.path() {
        vars.full_current_path = path.display().to_string();
        vars.current_directory = path
            .parent()
            .map(|dir| dir.display().to_string())
            .unwrap_or_default();
    }
    vars.file_name = buffer.display_name();
    let text = buffer.doc().text();
    let primary = view.selection.primary();
    let line = line_of(text, primary.head);
    let bounds = line_range(text, line);
    vars.current_line = line + 1;
    vars.current_column = text.slice(bounds.start..primary.head).chars().count() + 1;
    vars.current_line_text = text.slice(bounds.clone()).to_string();
    vars.current_word = if primary.is_empty() {
        let word = word_at(text, primary.head);
        text.slice(word).to_string()
    } else {
        let selected = text.slice(primary.from()..primary.to()).to_string();
        if selected.contains(['\n', '\r']) {
            String::new()
        } else {
            selected
        }
    };
    vars
}

/// Expands `line` with `vars` and runs it.
pub(crate) fn launch(line: &str, vars: RunVariables, cx: &mut App) -> Result<()> {
    if line.trim().is_empty() {
        bail!("{}", t!("run-menu.there-is-no-command-to-run"));
    }
    let expanded =
        birchpad_commands::expand_variables(line, &vars, |name| std::env::var(name).ok());
    run_menu(cx).last = line.to_owned();
    if is_url(&expanded) {
        cx.open_url(expanded.trim());
        return Ok(());
    }
    let mut command = shell_command(&expanded);
    if !vars.current_directory.is_empty() {
        command.current_dir(&vars.current_directory);
    }
    command
        .stdin(std::process::Stdio::null())
        .spawn()
        .with_context(|| format!("cannot run {expanded}"))?;
    Ok(())
}

/// Saves `line` as `name` in `run.toml`, replacing a command of that name.
pub(crate) fn save(name: &str, line: &str, cx: &mut App) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        bail!("{}", t!("run-menu.a-command-needs-a-name"));
    }
    if line.trim().is_empty() {
        bail!("{}", t!("run-menu.there-is-no-command-to-save"));
    }
    let path = run_path(cx).ok_or_else(|| {
        anyhow!(
            "{}",
            t!("run-menu.there-is-no-settings-folder-for-run-toml")
        )
    })?;
    let mut saved = run_menu(cx).saved.clone();
    let entry = RunEntry {
        name: name.to_owned(),
        line: line.to_owned(),
    };
    match saved.iter_mut().find(|known| known.name == name) {
        Some(known) => *known = entry,
        None => saved.push(entry),
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }
    std::fs::write(&path, birchpad_commands::write_run_commands(&saved))
        .with_context(|| format!("cannot write {}", path.display()))?;
    run_menu(cx).saved = saved;
    Ok(())
}

/// Deletes the saved command `name` from `run.toml` (Shortcut Mapper > Run > Delete).
pub(crate) fn delete(name: &str, cx: &mut App) -> Result<()> {
    let path = run_path(cx).ok_or_else(|| {
        anyhow!(
            "{}",
            t!("run-menu.there-is-no-settings-folder-for-run-toml")
        )
    })?;
    let mut saved = run_menu(cx).saved.clone();
    let before = saved.len();
    saved.retain(|known| known.name != name);
    if saved.len() == before {
        bail!("{}", t!("run-menu.there-is-no-command-named", name = name));
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }
    std::fs::write(&path, birchpad_commands::write_run_commands(&saved))
        .with_context(|| format!("cannot write {}", path.display()))?;
    run_menu(cx).saved = saved;
    Ok(())
}

#[derive(Deserialize)]
struct RunArgs {
    /// The command line; unset asks for it.
    #[serde(default)]
    line: Option<String>,
    /// A saved command, by name.
    #[serde(default)]
    name: Option<String>,
}

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("run.run", |this, args: RunArgs, window, cx| {
        let line = match (args.line, args.name) {
            (Some(line), _) => line,
            (None, Some(name)) => run_menu(cx)
                .saved
                .iter()
                .find(|known| known.name == name)
                .map(|known| known.line.clone())
                .ok_or_else(|| {
                    anyhow!(
                        "{}",
                        t!("run-menu.there-is-no-saved-command-named", name = name)
                    )
                })?,
            (None, None) => {
                open(cx.entity().downgrade(), window, cx);
                return Ok(());
            }
        };
        let vars = variables(this, cx);
        launch(&line, vars, cx)
    });
    registry.workspace("run.builtin", |this, args: BuiltinArgs, _, cx| {
        let line = birchpad_commands::builtin_run_commands()
            .into_iter()
            .find(|entry| entry.name == args.name)
            .map(|entry| entry.line)
            .ok_or_else(|| {
                anyhow!(
                    "{}",
                    t!(
                        "run-menu.there-is-no-built-in-command-named",
                        name = args.name
                    )
                )
            })?;
        let vars = variables(this, cx);
        launch(&line, vars, cx)
    });
}

#[derive(Deserialize)]
struct BuiltinArgs {
    name: String,
}

/// The Run... dialog.
pub(crate) struct RunDialog {
    workspace: WeakEntity<Workspace>,
    line: Entity<InputState>,
    name: Entity<InputState>,
    error: Option<String>,
}

fn open(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut App) {
    let dialog = cx.new(|cx| RunDialog::new(workspace, window, cx));
    let focus = dialog.read(cx).line.clone();
    window.open_dialog(cx, move |modal, _, _| {
        let run = dialog.clone();
        modal
            .title(t!("run-menu.run"))
            .w(px(560.))
            .child(dialog.clone())
            .footer(
                DialogFooter::new()
                    .child(crate::workspace::dialog_close("common.cancel"))
                    .child(crate::workspace::dialog_action(
                        Button::new("run-run").label(t!("common.run")),
                    )),
            )
            .on_ok(move |_, _, cx| run.update(cx, |this, cx| this.run(cx)))
    });
    focus.update(cx, |input, cx| input.focus(window, cx));
}

impl RunDialog {
    fn new(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let last = run_menu(cx).last.clone();
        Self {
            workspace,
            line: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(
                        "The program to run, with $(FULL_CURRENT_PATH) and other variables",
                    )
                    .default_value(last)
            }),
            name: cx.new(|cx| {
                InputState::new(window, cx).placeholder(t!("run-menu.name-to-save-it-as"))
            }),
            error: None,
        }
    }

    /// Runs the line; false keeps the dialog open to show what went wrong.
    pub(crate) fn run(&mut self, cx: &mut Context<Self>) -> bool {
        let line = self.line.read(cx).value().to_string();
        let result = self
            .workspace
            .upgrade()
            .ok_or_else(|| anyhow!("{}", t!("run-menu.the-window-is-closed")))
            .and_then(|workspace| {
                let vars = variables(workspace.read(cx), cx);
                launch(&line, vars, cx)
            });
        self.finish(result, cx)
    }

    pub(crate) fn save(&mut self, cx: &mut Context<Self>) {
        let line = self.line.read(cx).value().to_string();
        let name = self.name.read(cx).value().to_string();
        let result = save(&name, &line, cx);
        if result.is_ok() {
            self.workspace
                .update(cx, |workspace, cx| workspace.refresh_menus(cx))
                .ok();
        }
        self.finish(result, cx);
    }

    fn finish(&mut self, result: Result<()>, cx: &mut Context<Self>) -> bool {
        self.error = result.as_ref().err().map(|error| format!("{error:#}"));
        cx.notify();
        result.is_ok()
    }
}

impl Render for RunDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(Input::new(&self.line).id("run-line"))
            .child(
                div()
                    .text_sm()
                    .text_color(paint(ui().muted))
                    .child("$(FULL_CURRENT_PATH) $(CURRENT_DIRECTORY) $(FILE_NAME) $(NAME_PART) $(EXT_PART) $(CURRENT_WORD) $(CURRENT_LINE) $(CURRENT_COLUMN) $(CURRENT_LINESTR) $(NPP_DIRECTORY) $(NPP_FULL_FILE_PATH) $(SYS.NAME)"),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().child(Input::new(&self.name).id("run-name")))
                    .child(
                        Button::new("run-save")
                            .label(t!("common.save"))
                            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                    ),
            )
            .children(
                self.error
                    .clone()
                    .map(|error| div().text_color(paint(ui().error)).child(error)),
            )
    }
}

#[cfg(test)]
mod tests;
