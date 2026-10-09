//! Settings > Shortcut Mapper..., as in Notepad++: every command with its shortcuts, which can be
//! added (by pressing the keys), removed and reset to the defaults. Changes go to the user's
//! `keymap.toml`, keeping the rest of it, and take effect at once.

use anyhow::{Context as _, Result, anyhow};
use birchpad_commands::{Invocation, Keymap, Keystroke, MenuItem, Platform, Scope};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Disableable as _, WindowExt as _};
use gpui_kit::{
    App, AppContext as _, Context, Entity, Subscription, WeakEntity, Window, div, prelude::*, px,
    uniform_list,
};

use crate::app_state::AppState;
use crate::commands::{ActiveKeymap, CommandRegistry};
use crate::theme::{paint, ui};
use crate::workspace::Workspace;

/// A command as the mapper lists it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Row {
    pub(crate) invocation: Invocation,
    pub(crate) name: String,
    /// The group of its id: `file`, `edit`, ...
    pub(crate) category: String,
}

/// Every command of the catalog, then the menu items and bindings that run one with arguments
/// (Convert Case to UPPERCASE), each once.
pub(crate) fn rows(keymap: &Keymap) -> Vec<Row> {
    let category = |id: &str| id.split('.').next().unwrap_or_default().to_owned();
    let mut rows: Vec<Row> = birchpad_commands::COMMANDS
        .iter()
        .map(|spec| Row {
            invocation: Invocation::new(spec.id),
            name: spec.title.to_owned(),
            category: category(spec.id),
        })
        .collect();
    let mut with_args = Vec::new();
    fn walk(items: &[MenuItem], parent: &str, out: &mut Vec<(Invocation, String)>) {
        for item in items {
            match item {
                MenuItem::Command { invocation, .. } if !invocation.args.is_null() => {
                    let label = item.label().unwrap_or_default();
                    out.push((invocation.clone(), format!("{parent}: {label}")));
                }
                MenuItem::Submenu(menu) => walk(&menu.items, &menu.title, out),
                _ => {}
            }
        }
    }
    for menu in birchpad_commands::main_menu() {
        walk(&menu.items, &menu.title, &mut with_args);
    }
    for binding in keymap.bindings() {
        if !binding.invocation.args.is_null() {
            let title = birchpad_commands::find(&binding.invocation.command)
                .map_or(binding.invocation.command.as_str(), |spec| spec.title);
            with_args.push((
                binding.invocation.clone(),
                format!("{title} {}", binding.invocation.args),
            ));
        }
    }
    for (invocation, name) in with_args {
        if !rows.iter().any(|row| row.invocation == invocation) {
            rows.push(Row {
                category: category(&invocation.command),
                invocation,
                name,
            });
        }
    }
    rows
}

/// The shortcuts of `invocation`, as keymap files write them.
pub(crate) fn shortcuts(keymap: &Keymap, invocation: &Invocation) -> Vec<String> {
    keymap
        .bindings_for(invocation)
        .map(birchpad_commands::Binding::keys_string)
        .collect()
}

/// Whether `row` matches a filter: its name, id or shortcuts contain it, ignoring case.
pub(crate) fn matches(row: &Row, shortcuts: &[String], filter: &str) -> bool {
    let filter = filter.trim().to_lowercase();
    filter.is_empty()
        || row.name.to_lowercase().contains(&filter)
        || row.invocation.command.contains(&filter)
        || shortcuts.iter().any(|keys| keys.contains(&filter))
}

/// The context a command's new shortcuts are bound in: editor commands only in the editor.
fn context_of(invocation: &Invocation) -> Option<&'static str> {
    birchpad_commands::find(&invocation.command)
        .filter(|spec| spec.scope == Scope::Editor)
        .map(|_| "Editor")
}

/// What `keys` run now in the context a new shortcut of `invocation` would get, if anything
/// else.
pub(crate) fn conflict(
    keymap: &Keymap,
    keys: &[Keystroke],
    invocation: &Invocation,
) -> Option<Invocation> {
    let context = context_of(invocation);
    keymap
        .bindings()
        .iter()
        .find(|binding| binding.keys == keys && binding.context.as_deref() == context)
        .map(|binding| binding.invocation.clone())
        .filter(|found| found != invocation)
}

/// The user's keymap file.
fn keymap_path(cx: &App) -> Option<std::path::PathBuf> {
    AppState::global(cx)
        .paths
        .user_config_dir()
        .map(|dir| dir.join("keymap.toml"))
}

/// Changes the user's keymap file with `edit` and takes the result at once.
pub(crate) fn change(
    cx: &mut App,
    edit: impl FnOnce(&str) -> Result<String, birchpad_commands::KeymapError>,
) -> Result<()> {
    let path =
        keymap_path(cx).ok_or_else(|| anyhow!("there is no settings folder for keymap.toml"))?;
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error).with_context(|| format!("cannot read {}", path.display())),
    };
    let text = edit(&text).with_context(|| format!("{}", path.display()))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }
    std::fs::write(&path, &text).with_context(|| format!("cannot write {}", path.display()))?;
    crate::commands::reload_keymap(Some(&text), cx);
    Ok(())
}

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("settings.shortcut-mapper", |_, (), window, cx| {
        open(cx.entity().downgrade(), window, cx);
        Ok(())
    });
}

pub(crate) struct ShortcutMapper {
    workspace: WeakEntity<Workspace>,
    rows: Vec<Row>,
    filter: Entity<InputState>,
    /// Indexes into `rows` of those the filter shows.
    shown: Vec<usize>,
    selected: Option<usize>,
    /// Waiting for the keys of a new shortcut; the keys pressed are taken, not run.
    capture: Option<Subscription>,
    /// The keys pressed for a new shortcut.
    pressed: Option<Vec<Keystroke>>,
    error: Option<String>,
}

/// Opens the dialog.
pub(crate) fn open(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut App) {
    let mapper = cx.new(|cx| ShortcutMapper::new(workspace, window, cx));
    let focus = mapper.read(cx).filter.clone();
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .title("Shortcut Mapper")
            .w(px(760.))
            .child(mapper.clone())
            .footer(DialogFooter::new().child(crate::workspace::dialog_close("Close")))
    });
    focus.update(cx, |input, cx| input.focus(window, cx));
}

impl ShortcutMapper {
    fn new(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter"));
        cx.subscribe(&filter, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.refilter(cx);
            }
        })
        .detach();
        let rows = rows(&cx.global::<ActiveKeymap>().0);
        let mut this = Self {
            workspace,
            shown: (0..rows.len()).collect(),
            rows,
            filter,
            selected: None,
            capture: None,
            pressed: None,
            error: None,
        };
        this.refilter(cx);
        this
    }

    fn keymap<'a>(&self, cx: &'a App) -> &'a Keymap {
        &cx.global::<ActiveKeymap>().0
    }

    fn refilter(&mut self, cx: &mut Context<Self>) {
        let filter = self.filter.read(cx).value().to_string();
        let keymap = self.keymap(cx);
        self.shown = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| matches(row, &shortcuts(keymap, &row.invocation), &filter))
            .map(|(index, _)| index)
            .collect();
        cx.notify();
    }

    fn select(&mut self, index: usize, cx: &mut Context<Self>) {
        self.selected = Some(index);
        self.stop_capture();
        self.error = None;
        cx.notify();
    }

    fn selected_row(&self) -> Option<&Row> {
        self.selected.map(|index| &self.rows[index])
    }

    /// Waits for the keys of a new shortcut.
    fn start_capture(&mut self, cx: &mut Context<Self>) {
        let this = cx.entity().downgrade();
        self.pressed = None;
        self.error = None;
        self.capture = Some(cx.intercept_keystrokes(move |event, _, cx| {
            let keystroke = event.keystroke.unparse();
            if this
                .update(cx, |this, cx| this.pressed_keys(&keystroke, cx))
                .is_ok()
            {
                cx.stop_propagation();
            }
        }));
        cx.notify();
    }

    fn stop_capture(&mut self) {
        self.capture = None;
        self.pressed = None;
    }

    /// A key pressed while waiting for a shortcut: Escape gives up.
    pub(crate) fn pressed_keys(&mut self, keystroke: &str, cx: &mut Context<Self>) {
        if keystroke == "escape" {
            self.stop_capture();
        } else {
            match Keystroke::parse(keystroke, Platform::current()) {
                Ok(keys) => {
                    self.pressed = Some(vec![keys]);
                    self.error = None;
                }
                Err(error) => self.error = Some(error.to_string()),
            }
        }
        cx.notify();
    }

    fn done(&mut self, result: Result<()>, cx: &mut Context<Self>) {
        match result {
            Ok(()) => {
                self.error = None;
                self.workspace
                    .update(cx, |workspace, cx| workspace.refresh_menus(cx))
                    .ok();
            }
            Err(error) => self.error = Some(format!("{error:#}")),
        }
        self.refilter(cx);
    }

    /// Binds the keys pressed to the selected command.
    pub(crate) fn assign(&mut self, cx: &mut Context<Self>) {
        let (Some(row), Some(keys)) = (self.selected_row().cloned(), self.pressed.clone()) else {
            return;
        };
        self.stop_capture();
        let text: Vec<String> = keys.iter().map(Keystroke::to_string).collect();
        let context = context_of(&row.invocation);
        let result = change(cx, |file| {
            birchpad_commands::bind(
                file,
                Platform::current(),
                &text.join(" "),
                context,
                &row.invocation,
            )
        });
        self.done(result, cx);
    }

    /// Takes `keys` off the selected command.
    pub(crate) fn remove(&mut self, keys: &str, cx: &mut Context<Self>) {
        let Some(row) = self.selected_row().cloned() else {
            return;
        };
        let keymap = self.keymap(cx);
        let context = keymap
            .bindings_for(&row.invocation)
            .find(|binding| binding.keys_string() == keys)
            .and_then(|binding| binding.context.clone());
        let keys = keys.to_owned();
        let result = change(cx, |file| {
            birchpad_commands::unbind(file, Platform::current(), &keys, context.as_deref())
        });
        self.done(result, cx);
    }

    /// Gives the selected command its default shortcuts back.
    pub(crate) fn reset(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.selected_row().cloned() else {
            return;
        };
        let result = change(cx, |file| {
            birchpad_commands::reset(file, Platform::current(), &row.invocation)
        });
        self.done(result, cx);
    }
}

impl Render for ShortcutMapper {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = ui();
        let keymap = self.keymap(cx).clone();
        let this = cx.entity().downgrade();
        let rows = self.rows.clone();
        let shown = self.shown.clone();
        let selected = self.selected;
        let list = uniform_list("shortcut-rows", shown.len(), move |range, _, _| {
            range
                .map(|position| {
                    let index = shown[position];
                    let row = &rows[index];
                    let keys = shortcuts(&keymap, &row.invocation).join(", ");
                    let this = this.clone();
                    div()
                        .id(("shortcut-row", index))
                        .flex()
                        .flex_row()
                        .px_2()
                        .h(px(22.))
                        .items_center()
                        .when(selected == Some(index), |row| {
                            row.bg(paint(colors.selected))
                        })
                        .hover(|row| row.bg(paint(colors.hovered)))
                        .child(div().w(px(300.)).truncate().child(row.name.clone()))
                        .child(
                            div()
                                .w(px(90.))
                                .text_color(paint(colors.muted))
                                .child(row.category.clone()),
                        )
                        .child(div().flex_1().truncate().child(keys))
                        .on_click(move |_, _, cx| {
                            this.update(cx, |this, cx| this.select(index, cx)).ok();
                        })
                })
                .collect()
        })
        .h(px(320.));
        let keymap = self.keymap(cx);
        let detail = self.selected_row().map(|row| {
            let current = shortcuts(keymap, &row.invocation);
            let conflict = self
                .pressed
                .as_deref()
                .and_then(|keys| conflict(keymap, keys, &row.invocation));
            let removes = current.into_iter().enumerate().map(|(index, keys)| {
                let label = keys.clone();
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(div().w(px(160.)).child(label))
                    .child(
                        Button::new(("shortcut-remove", index))
                            .label("Remove")
                            .on_click(cx.listener(move |this, _, _, cx| this.remove(&keys, cx))),
                    )
            });
            let capture = if self.capture.is_some() {
                let pressed = self.pressed.as_ref().map(|keys| {
                    keys.iter()
                        .map(Keystroke::to_string)
                        .collect::<Vec<_>>()
                        .join(" ")
                });
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .border_1()
                            .border_color(paint(colors.accent))
                            .child(
                                pressed
                                    .clone()
                                    .unwrap_or_else(|| "Press the keys (Esc: cancel)".into()),
                            ),
                    )
                    .child(
                        Button::new("shortcut-assign")
                            .primary()
                            .label("Assign")
                            .disabled(pressed.is_none())
                            .on_click(cx.listener(|this, _, _, cx| this.assign(cx))),
                    )
                    .into_any_element()
            } else {
                Button::new("shortcut-add")
                    .label("Add Shortcut...")
                    .on_click(cx.listener(|this, _, _, cx| this.start_capture(cx)))
                    .into_any_element()
            };
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(div().child(format!("{} ({})", row.name, row.invocation.command)))
                .children(removes)
                .child(
                    div().flex().flex_row().gap_2().child(capture).child(
                        Button::new("shortcut-reset")
                            .label("Reset")
                            .on_click(cx.listener(|this, _, _, cx| this.reset(cx))),
                    ),
                )
                .children(conflict.map(|other| {
                    let title = birchpad_commands::find(&other.command)
                        .map_or(other.command.clone(), |spec| spec.title.to_owned());
                    div().text_color(paint(colors.error)).child(format!(
                        "These keys run {title} now; assigning them replaces it."
                    ))
                }))
        });
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(Input::new(&self.filter).id("shortcut-filter"))
            .child(
                div()
                    .border_1()
                    .border_color(paint(colors.border))
                    .text_sm()
                    .child(list),
            )
            .children(detail)
            .children(
                self.error
                    .clone()
                    .map(|error| div().text_color(paint(colors.error)).child(error)),
            )
    }
}

#[cfg(test)]
mod tests;
