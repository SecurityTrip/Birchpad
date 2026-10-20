//! Settings > Shortcut Mapper..., as in Notepad++: every command with its shortcuts, which can be
//! added (by pressing the keys, a sequence such as Ctrl+K Ctrl+C too), for every platform or
//! this one only, removed and reset to the defaults; the Macros and Run tabs do the same for the
//! saved macros and command lines, and delete them. Changes go to the user's `keymap.toml`,
//! keeping the rest of it, and take effect at once.

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
use crate::commands::{ActiveKeymap, CommandRegistry, Register as _};
use crate::theme::{paint, ui};
use crate::workspace::Workspace;

/// A command as the mapper lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub invocation: Invocation,
    pub name: String,
    /// The group of its id: `file`, `edit`, ...
    pub category: String,
}

/// Every command of the catalog, then the menu items and bindings that run one with arguments
/// (Convert Case to UPPERCASE), each once.
pub fn rows(keymap: &Keymap) -> Vec<Row> {
    let category = |id: &str| id.split('.').next().unwrap_or_default().to_owned();
    let mut rows: Vec<Row> = birchpad_commands::COMMANDS
        .iter()
        .map(|spec| Row {
            invocation: Invocation::new(spec.id),
            name: crate::menus::command_title(spec.id),
            category: category(spec.id),
        })
        .collect();
    let mut with_args = Vec::new();
    fn walk(items: &[MenuItem], parent: &str, out: &mut Vec<(Invocation, String)>) {
        for item in items {
            match item {
                MenuItem::Command { invocation, .. } if !invocation.args.is_null() => {
                    let parent = crate::i18n::get_or(&crate::menus::menu_key(parent), parent);
                    out.push((
                        invocation.clone(),
                        format!("{parent}: {}", crate::menus::menu_label(item)),
                    ));
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
                .map_or(binding.invocation.command.clone(), |spec| {
                    crate::menus::command_title(spec.id)
                });
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

/// The tabs of the mapper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapperTab {
    Commands,
    Macros,
    Run,
}

impl MapperTab {
    pub const ALL: [Self; 3] = [Self::Commands, Self::Macros, Self::Run];
}

/// The rows of a tab: every command, or the saved macros (`macro.run`) or command lines
/// (`run.run`).
pub fn rows_of(tab: MapperTab, keymap: &Keymap, cx: &App) -> Vec<Row> {
    let named = |command: &str, names: Vec<String>| {
        names
            .into_iter()
            .map(|name| Row {
                invocation: Invocation::with_args(command, serde_json::json!({ "name": name })),
                category: command.split('.').next().unwrap_or_default().to_owned(),
                name,
            })
            .collect()
    };
    match tab {
        MapperTab::Commands => rows(keymap),
        MapperTab::Macros => named(
            "macro.run",
            cx.try_global::<crate::macros::Macros>()
                .map(|macros| {
                    macros
                        .saved
                        .iter()
                        .map(|found| found.name.clone())
                        .collect()
                })
                .unwrap_or_default(),
        ),
        MapperTab::Run => named(
            "run.run",
            cx.try_global::<crate::run_menu::RunMenu>()
                .map(|menu| menu.saved.iter().map(|entry| entry.name.clone()).collect())
                .unwrap_or_default(),
        ),
    }
}

/// The shortcuts of `invocation` as the list shows them: those limited to some platforms say
/// which.
pub fn shortcut_labels(keymap: &Keymap, invocation: &Invocation) -> Vec<String> {
    keymap
        .bindings_for(invocation)
        .map(|binding| match &binding.platforms {
            Some(platforms) => format!("{} ({})", binding.keys_string(), platforms.join(", ")),
            None => binding.keys_string(),
        })
        .collect()
}

/// The longest key sequence the mapper records.
pub const MAX_SEQUENCE: usize = 4;

/// The shortcuts of `invocation`, as keymap files write them.
pub fn shortcuts(keymap: &Keymap, invocation: &Invocation) -> Vec<String> {
    keymap
        .bindings_for(invocation)
        .map(birchpad_commands::Binding::keys_string)
        .collect()
}

/// Whether `row` matches a filter: its name, id or shortcuts contain it, ignoring case.
pub fn matches(row: &Row, shortcuts: &[String], filter: &str) -> bool {
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
pub fn conflict(
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
pub fn change(
    cx: &mut App,
    edit: impl FnOnce(&str) -> Result<String, birchpad_commands::KeymapError>,
) -> Result<()> {
    let path = keymap_path(cx).ok_or_else(|| {
        anyhow!(
            "{}",
            t!("shortcut-mapper.there-is-no-settings-folder-for-keymap-toml")
        )
    })?;
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

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("settings.shortcut-mapper", |_, (), window, cx| {
        open(cx.entity().downgrade(), window, cx);
        Ok(())
    });
}

pub struct ShortcutMapper {
    workspace: WeakEntity<Workspace>,
    pub tab: MapperTab,
    /// New shortcuts only on this platform.
    pub only_here: bool,
    pub rows: Vec<Row>,
    pub filter: Entity<InputState>,
    /// Indexes into `rows` of those the filter shows.
    pub shown: Vec<usize>,
    selected: Option<usize>,
    /// Waiting for the keys of a new shortcut; the keys pressed are taken, not run.
    pub capture: Option<Subscription>,
    /// The keys pressed for a new shortcut.
    pub pressed: Option<Vec<Keystroke>>,
    pub error: Option<String>,
}

#[cfg(feature = "test-support")]
thread_local! {
    /// The mapper last opened, for tests to drive.
    pub static OPENED: std::cell::RefCell<Option<WeakEntity<ShortcutMapper>>> =
        std::cell::RefCell::default();
}

/// Opens the dialog.
pub fn open(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut App) {
    let mapper = cx.new(|cx| ShortcutMapper::new(workspace, window, cx));
    #[cfg(feature = "test-support")]
    OPENED.with(|opened| *opened.borrow_mut() = Some(mapper.downgrade()));
    let focus = mapper.read(cx).filter.clone();
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .title(t!("shortcut-mapper.shortcut-mapper"))
            .w(px(760.))
            .child(mapper.clone())
            .footer(DialogFooter::new().child(crate::workspace::dialog_close("common.close")))
    });
    focus.update(cx, |input, cx| input.focus(window, cx));
}

impl ShortcutMapper {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let filter =
            cx.new(|cx| InputState::new(window, cx).placeholder(t!("shortcut-mapper.filter")));
        cx.subscribe(&filter, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.refilter(cx);
            }
        })
        .detach();
        let rows = rows(&cx.global::<ActiveKeymap>().0);
        let mut this = Self {
            workspace,
            tab: MapperTab::Commands,
            only_here: false,
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

    pub fn refilter(&mut self, cx: &mut Context<Self>) {
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

    /// Shows another tab.
    pub fn select_tab(&mut self, tab: MapperTab, cx: &mut Context<Self>) {
        self.tab = tab;
        self.rows = rows_of(tab, &cx.global::<ActiveKeymap>().0, cx);
        self.selected = None;
        self.stop_capture();
        self.error = None;
        self.refilter(cx);
    }

    /// Macros and Run > Delete: the selected macro or command line, and its shortcuts.
    pub fn delete_selected(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.selected_row().cloned() else {
            return;
        };
        let name = row
            .invocation
            .args
            .get("name")
            .and_then(|name| name.as_str())
            .unwrap_or_default()
            .to_owned();
        let keys = shortcuts(self.keymap(cx), &row.invocation);
        let mut result = match self.tab {
            MapperTab::Macros => crate::macros::delete(&name, cx),
            MapperTab::Run => crate::run_menu::delete(&name, cx),
            MapperTab::Commands => return,
        };
        for keys in keys {
            if result.is_ok() {
                let context = context_of(&row.invocation);
                result = change(cx, |file| {
                    birchpad_commands::unbind(file, Platform::current(), &keys, context)
                });
            }
        }
        let tab = self.tab;
        self.done(result, cx);
        self.select_tab(tab, cx);
    }

    pub fn select(&mut self, index: usize, cx: &mut Context<Self>) {
        self.selected = Some(index);
        self.stop_capture();
        self.error = None;
        cx.notify();
    }

    pub fn selected_row(&self) -> Option<&Row> {
        self.selected.map(|index| &self.rows[index])
    }

    /// Waits for the keys of a new shortcut.
    pub fn start_capture(&mut self, cx: &mut Context<Self>) {
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
    pub fn pressed_keys(&mut self, keystroke: &str, cx: &mut Context<Self>) {
        if keystroke == "escape" {
            self.stop_capture();
        } else {
            match Keystroke::parse(keystroke, Platform::current()) {
                Ok(keys) => {
                    // Keys pressed one after the other make a sequence (Ctrl+K Ctrl+C).
                    let pressed = self.pressed.get_or_insert_with(Vec::new);
                    if pressed.len() >= MAX_SEQUENCE {
                        pressed.clear();
                    }
                    pressed.push(keys);
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
    pub fn assign(&mut self, cx: &mut Context<Self>) {
        let (Some(row), Some(keys)) = (self.selected_row().cloned(), self.pressed.clone()) else {
            return;
        };
        self.stop_capture();
        let text: Vec<String> = keys.iter().map(Keystroke::to_string).collect();
        let context = context_of(&row.invocation);
        let only_here = self.only_here;
        let result = change(cx, |file| {
            birchpad_commands::bind_on(
                file,
                Platform::current(),
                &text.join(" "),
                context,
                &row.invocation,
                only_here,
            )
        });
        self.done(result, cx);
    }

    /// Takes `keys` off the selected command.
    pub fn remove(&mut self, keys: &str, cx: &mut Context<Self>) {
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
    pub fn reset(&mut self, cx: &mut Context<Self>) {
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
                    let keys = shortcut_labels(&keymap, &row.invocation).join(", ");
                    let this = this.clone();
                    div()
                        .id(("shortcut-row", index))
                        .debug_selector(move || format!("shortcut-row-{position}"))
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
                            .debug_selector(move || format!("shortcut-remove-{index}"))
                            .label(t!("common.remove"))
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
                            .child(pressed.clone().unwrap_or_else(|| {
                                t!("shortcut-mapper.press-the-keys-esc-cancel")
                            })),
                    )
                    .child(
                        Button::new("shortcut-assign")
                            .debug_selector(|| "shortcut-assign".into())
                            .primary()
                            .label(t!("shortcut-mapper.assign"))
                            .disabled(pressed.is_none())
                            .on_click(cx.listener(|this, _, _, cx| this.assign(cx))),
                    )
                    .into_any_element()
            } else {
                Button::new("shortcut-add")
                    .debug_selector(|| "shortcut-add".into())
                    .label(t!("shortcut-mapper.add-shortcut"))
                    .on_click(cx.listener(|this, _, _, cx| this.start_capture(cx)))
                    .into_any_element()
            };
            let deletable = self.tab != MapperTab::Commands;
            let only_here = self.only_here;
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(div().child(format!("{} ({})", row.name, row.invocation.command)))
                .children(removes)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .child(capture)
                        .child(
                            Button::new("shortcut-reset")
                                .debug_selector(|| "shortcut-reset".into())
                                .label(t!("shortcut-mapper.reset"))
                                .on_click(cx.listener(|this, _, _, cx| this.reset(cx))),
                        )
                        .when(deletable, |row| {
                            row.child(
                                Button::new("shortcut-delete")
                                    .debug_selector(|| "shortcut-delete".into())
                                    .label(t!("shortcut-mapper.delete"))
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.delete_selected(cx)),
                                    ),
                            )
                        }),
                )
                .child(
                    gpui_kit::component::checkbox::Checkbox::new("shortcut-only-here")
                        .debug_selector(|| "shortcut-only-here".into())
                        .label(t!(
                            "shortcut-mapper.only-on",
                            platform = Platform::current().name()
                        ))
                        .checked(only_here)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            this.only_here = *checked;
                            cx.notify();
                        })),
                )
                .children(conflict.map(|other| {
                    let title = birchpad_commands::find(&other.command)
                        .map_or(other.command.clone(), |spec| {
                            crate::menus::command_title(spec.id)
                        });
                    div()
                        .debug_selector(|| "shortcut-conflict".into())
                        .text_color(paint(colors.error))
                        .child(t!("shortcut-mapper.conflict", title = title))
                }))
        });
        let current_tab = self.tab;
        let tabs = gpui_kit::component::tab::TabBar::new("shortcut-tabs")
            .underline()
            .selected_index(
                MapperTab::ALL
                    .iter()
                    .position(|tab| *tab == current_tab)
                    .unwrap_or(0),
            )
            .on_click(cx.listener(|this, index: &usize, _, cx| {
                this.select_tab(MapperTab::ALL[*index], cx);
            }))
            .child(
                gpui_kit::component::tab::Tab::new()
                    .debug_selector(|| "shortcut-tab-0".into())
                    .label(t!("shortcut-mapper.commands")),
            )
            .child(
                gpui_kit::component::tab::Tab::new()
                    .debug_selector(|| "shortcut-tab-1".into())
                    .label(t!("shortcut-mapper.macros")),
            )
            .child(
                gpui_kit::component::tab::Tab::new()
                    .debug_selector(|| "shortcut-tab-2".into())
                    .label(t!("shortcut-mapper.run-commands")),
            );
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(tabs)
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
