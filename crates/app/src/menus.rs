//! Builds the main menu from the menu model in `birchpad-commands`.
//!
//! macOS gets a native menu bar through `cx.set_menus`; Windows and Linux get gpui-component's
//! `AppMenuBar`, drawn at the top of the window, fed from the same menus.

use std::path::PathBuf;

use birchpad_commands::{Invocation, Menu, MenuItem, Placeholder};
use gpui_kit::component::GlobalState;
use gpui_kit::{App, MenuItem as GpuiMenuItem};
use serde_json::json;

use crate::commands::{CommandRegistry, RunCommand};

/// Live state the menus reflect.
pub(crate) struct MenuState<'a> {
    pub(crate) recent_files: &'a [PathBuf],
    /// Legacy encodings grouped by script, as `(group, [(name, label)])`.
    pub(crate) character_sets: &'a [(&'a str, Vec<(&'a str, &'a str)>)],
    /// Whether a command item shows a check mark (word wrap, current encoding, ...).
    pub(crate) checked: &'a dyn Fn(&Invocation) -> bool,
}

/// Turns the menu model into GPUI menus.
fn build(model: Vec<Menu>, state: &MenuState, cx: &App) -> Vec<gpui_kit::Menu> {
    let registry = cx.global::<CommandRegistry>();
    let model = if cfg!(target_os = "macos") {
        mac_layout(model)
    } else {
        model
    };
    model
        .into_iter()
        .map(|menu| build_menu(menu, state, registry))
        .collect()
}

fn build_menu(menu: Menu, state: &MenuState, registry: &CommandRegistry) -> gpui_kit::Menu {
    let mut items = Vec::new();
    for item in menu.items {
        match item {
            MenuItem::Command { .. } => {
                let label = item.label().unwrap_or_default().to_owned();
                let MenuItem::Command { invocation, .. } = item else {
                    unreachable!()
                };
                let disabled = registry.is_pending(&invocation.command);
                let checked = (state.checked)(&invocation);
                items.push(
                    GpuiMenuItem::action(label, RunCommand(invocation))
                        .checked(checked)
                        .disabled(disabled),
                );
            }
            MenuItem::Separator => items.push(GpuiMenuItem::separator()),
            MenuItem::Submenu(submenu) => {
                items.push(GpuiMenuItem::submenu(build_menu(submenu, state, registry)))
            }
            MenuItem::Placeholder(Placeholder::RecentFiles) => {
                if state.recent_files.is_empty() {
                    items.push(
                        GpuiMenuItem::action("(empty)", RunCommand::new("file.clear-recent"))
                            .disabled(true),
                    );
                }
                for (index, path) in state.recent_files.iter().enumerate() {
                    let label = format!("{}: {}", index + 1, path.display());
                    let invocation =
                        Invocation::with_args("file.open-recent", json!({ "path": path }));
                    items.push(GpuiMenuItem::action(label, RunCommand(invocation)));
                }
            }
            MenuItem::Placeholder(Placeholder::CharacterSets { command }) => {
                for (group, encodings) in state.character_sets {
                    let entries = encodings.iter().map(|(name, label)| {
                        let invocation =
                            Invocation::with_args(command, json!({ "encoding": name }));
                        let checked = (state.checked)(&invocation);
                        GpuiMenuItem::action(*label, RunCommand(invocation)).checked(checked)
                    });
                    items.push(GpuiMenuItem::submenu(
                        gpui_kit::Menu::new(group.to_string()).items(entries),
                    ));
                }
            }
        }
    }
    gpui_kit::Menu::new(menu.title).items(items)
}

/// On macOS, About, Check for Updates and Quit live in the application menu.
fn mac_layout(model: Vec<Menu>) -> Vec<Menu> {
    const APP_ITEMS: [&str; 3] = ["help.about", "help.check-updates", "file.exit"];
    let is_app_item = |item: &MenuItem| {
        matches!(item, MenuItem::Command { invocation, .. }
            if APP_ITEMS.contains(&invocation.command.as_str()))
    };
    let mut menus: Vec<Menu> = model
        .into_iter()
        .map(|mut menu| {
            menu.items.retain(|item| !is_app_item(item));
            // Drop separators left dangling at either end.
            while matches!(menu.items.last(), Some(MenuItem::Separator)) {
                menu.items.pop();
            }
            while matches!(menu.items.first(), Some(MenuItem::Separator)) {
                menu.items.remove(0);
            }
            menu
        })
        .filter(|menu| !menu.items.is_empty())
        .collect();
    let app = Menu::new(
        "Birchpad",
        vec![
            MenuItem::command("help.about"),
            MenuItem::command("help.check-updates"),
            MenuItem::Separator,
            MenuItem::Command {
                invocation: Invocation::new("file.exit"),
                label: Some("Quit Birchpad".into()),
            },
        ],
    );
    menus.insert(0, app);
    menus
}

/// Installs the menus: natively on macOS, for the drawn menu bar elsewhere.
pub(crate) fn install(state: &MenuState, cx: &mut App) {
    if cfg!(target_os = "macos") {
        let menus = build(birchpad_commands::main_menu(), state, cx);
        cx.set_menus(menus);
    } else {
        let owned = build(birchpad_commands::main_menu(), state, cx)
            .into_iter()
            .map(gpui_kit::Menu::owned)
            .collect();
        GlobalState::global_mut(cx).set_app_menus(owned);
    }
}
