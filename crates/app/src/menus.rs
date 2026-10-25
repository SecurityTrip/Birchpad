//! Builds the main menu from the menu model in `birchpad-commands`.
//!
//! macOS gets a native menu bar through `cx.set_menus`; Windows and Linux get the menu bar of
//! `birchpad-menu-bar`, drawn at the top of the window, fed from the same menus.

use std::path::PathBuf;

use birchpad_commands::{Invocation, Menu, MenuItem, Placeholder};
use gpui_kit::component::GlobalState;
use gpui_kit::{App, MenuItem as GpuiMenuItem};
use serde_json::json;

use crate::commands::{CommandRegistry, RunCommand};

/// Live state the menus reflect.
pub struct MenuState<'a> {
    pub recent_files: &'a [PathBuf],
    /// The user defined languages the Language menu lists.
    pub user_languages: &'a [String],
    /// The themes Settings > Theme lists.
    pub themes: &'a [String],
    /// The saved macros the Macro menu lists.
    pub macros: &'a [String],
    /// The saved command lines the Run menu lists.
    pub run_commands: &'a [String],
    /// The languages of the spell checker's dictionaries.
    pub spell_languages: &'a [String],
    /// The scripts of the `scripts` folder, with the commands they declare.
    pub scripts: &'a [(String, Vec<birchpad_plugin::script::ScriptCommand>)],
    /// The plugins ready to run: id, name and commands.
    pub plugins: &'a [(String, String, Vec<birchpad_plugin::PluginCommand>)],
    /// The documents of the active view, and which one is active (the Window menu).
    pub documents: &'a [String],
    pub active_document: Option<usize>,
    /// Whether a command item shows a check mark (word wrap, current encoding, ...).
    pub checked: &'a dyn Fn(&Invocation) -> bool,
}

/// The key of a menu's or a menu item's own title: `menu.<title>`.
pub fn menu_key(title: &str) -> String {
    format!("menu.{}", crate::i18n::slug(title))
}

/// The key of a command's title: `command.<id>`.
pub fn command_key(id: &str) -> String {
    format!("command.{id}")
}

/// A command's title in the interface's language.
pub fn command_title(id: &str) -> String {
    let english = birchpad_commands::find(id).map_or(id, |spec| spec.title);
    crate::i18n::get_or(&command_key(id), english)
}

/// The text of a menu item: its own label (`menu.<label>`), or its command's title.
pub fn menu_label(item: &MenuItem) -> String {
    match item {
        MenuItem::Command {
            label: Some(label), ..
        } => crate::i18n::get_or(&menu_key(label), label),
        MenuItem::Command { invocation, .. } => command_title(&invocation.command),
        MenuItem::Submenu(menu) => crate::i18n::get_or(&menu_key(&menu.title), &menu.title),
        _ => String::new(),
    }
}

/// How File > Recent Files names a file: by its whole path or its name alone, a long path
/// cut in its middle to the maximum length (Preferences > Recent Files History).
pub fn recent_label(path: &std::path::Path, files: &birchpad_config::FileSettings) -> String {
    let text = if files.recent_file_name_only {
        path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        )
    } else {
        path.display().to_string()
    };
    let max = usize::from(files.recent_max_length);
    let length = text.chars().count();
    if max == 0 || length <= max {
        return text;
    }
    // The start and the end, which says the most, around an ellipsis.
    let keep = max.saturating_sub(1).max(2);
    let head = keep / 2;
    let tail = keep - head;
    let start: String = text.chars().take(head).collect();
    let end: String = text.chars().skip(length - tail).collect();
    format!("{start}…{end}")
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
        .map(|menu| build_menu(menu, state, registry, cx))
        .collect()
}

fn build_menu(
    menu: Menu,
    state: &MenuState,
    registry: &CommandRegistry,
    cx: &App,
) -> gpui_kit::Menu {
    let mut items = Vec::new();
    for item in menu.items {
        match item {
            MenuItem::Command { ref invocation, .. } => {
                let label = menu_label(&item);
                let invocation = invocation.clone();
                let disabled = !registry.is_enabled(&invocation.command, cx);
                let checked = (state.checked)(&invocation);
                items.push(
                    GpuiMenuItem::action(label, RunCommand(invocation))
                        .checked(checked)
                        .disabled(disabled),
                );
            }
            MenuItem::Separator => items.push(GpuiMenuItem::separator()),
            MenuItem::Submenu(submenu) => items.push(GpuiMenuItem::submenu(build_menu(
                submenu, state, registry, cx,
            ))),
            MenuItem::Placeholder(Placeholder::RecentFiles) => {
                let files = &crate::app_state::AppState::global(cx).settings.files;
                let mut recent = Vec::new();
                let shown: Vec<&PathBuf> = state
                    .recent_files
                    .iter()
                    .filter(|path| !files.recent_check_existence || path.exists())
                    .collect();
                if shown.is_empty() {
                    recent.push(
                        GpuiMenuItem::action(
                            t!("menus.empty"),
                            RunCommand::new("file.clear-recent"),
                        )
                        .disabled(true),
                    );
                }
                for (index, path) in shown.into_iter().enumerate() {
                    let label = format!("{}: {}", index + 1, recent_label(path, files));
                    let invocation =
                        Invocation::with_args("file.open-recent", json!({ "path": path }));
                    recent.push(GpuiMenuItem::action(label, RunCommand(invocation)));
                }
                // Preferences > Recent Files History > In Submenu.
                if files.recent_submenu {
                    items.push(GpuiMenuItem::submenu(
                        gpui_kit::Menu::new(t!("menus.recent-files")).items(recent),
                    ));
                } else {
                    items.extend(recent);
                }
            }
            MenuItem::Placeholder(Placeholder::CharacterSets { command }) => {
                for (group, encodings) in birchpad_io::CHARACTER_SETS {
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
            MenuItem::Placeholder(Placeholder::Languages) => {
                // Preferences > Language: the languages not hidden, grouped by first letter as
                // in Notepad++ (compact), or one under the other.
                let settings = &crate::app_state::AppState::global(cx)
                    .settings
                    .language_menu;
                let shown: Vec<&birchpad_syntax::Language> = birchpad_syntax::LANGUAGES
                    .iter()
                    .filter(|language| settings.shows(language.id))
                    .collect();
                let item = |language: &birchpad_syntax::Language| {
                    let invocation =
                        Invocation::with_args("language.set", json!({ "language": language.id }));
                    let checked = (state.checked)(&invocation);
                    GpuiMenuItem::action(language.name, RunCommand(invocation)).checked(checked)
                };
                if !settings.compact {
                    items.extend(shown.iter().map(|language| item(language)));
                } else {
                    let mut letters: Vec<char> = shown
                        .iter()
                        .filter_map(|language| language.name.chars().next())
                        .map(|letter| letter.to_ascii_uppercase())
                        .collect();
                    letters.dedup();
                    for letter in letters {
                        let entries = shown
                            .iter()
                            .filter(|language| {
                                language.name.chars().next().map(|c| c.to_ascii_uppercase())
                                    == Some(letter)
                            })
                            .map(|language| item(language));
                        items.push(GpuiMenuItem::submenu(
                            gpui_kit::Menu::new(letter.to_string()).items(entries),
                        ));
                    }
                }
            }
            MenuItem::Placeholder(Placeholder::UserLanguages) => {
                for name in state.user_languages {
                    let invocation = Invocation::with_args(
                        "language.set",
                        json!({ "language": format!("{}{name}", crate::user_languages::ID_PREFIX) }),
                    );
                    let checked = (state.checked)(&invocation);
                    items.push(
                        GpuiMenuItem::action(name.clone(), RunCommand(invocation)).checked(checked),
                    );
                }
            }
            MenuItem::Placeholder(Placeholder::SpellLanguages) => {
                if state.spell_languages.is_empty() {
                    items.push(
                        GpuiMenuItem::action(
                            t!("menus.no-dictionaries"),
                            RunCommand::new("spell.open-dictionaries-folder"),
                        )
                        .disabled(true),
                    );
                }
                for language in state.spell_languages {
                    let invocation =
                        Invocation::with_args("spell.language", json!({ "language": language }));
                    let checked = (state.checked)(&invocation);
                    items.push(
                        GpuiMenuItem::action(language.clone(), RunCommand(invocation))
                            .checked(checked),
                    );
                }
            }
            MenuItem::Placeholder(Placeholder::Scripts) => {
                for (name, commands) in state.scripts {
                    let invocation = Invocation::with_args("script.run", json!({ "name": name }));
                    if commands.is_empty() {
                        items.push(GpuiMenuItem::action(name.clone(), RunCommand(invocation)));
                        continue;
                    }
                    // Run, then the script's own commands.
                    let entries = std::iter::once(GpuiMenuItem::action(
                        t!("menus.run-script"),
                        RunCommand(invocation),
                    ))
                    .chain(std::iter::once(GpuiMenuItem::separator()))
                    .chain(commands.iter().map(|command| {
                        let invocation = Invocation::with_args(
                            "script.run",
                            json!({ "name": name, "command": command.function }),
                        );
                        GpuiMenuItem::action(command.title.clone(), RunCommand(invocation))
                    }));
                    items.push(GpuiMenuItem::submenu(
                        gpui_kit::Menu::new(name.clone()).items(entries),
                    ));
                }
            }
            MenuItem::Placeholder(Placeholder::Plugins) => {
                for (id, name, commands) in state.plugins {
                    let entries = commands.iter().map(|command| {
                        let invocation = Invocation::with_args(
                            "plugin.run",
                            json!({ "plugin": id, "command": command.id }),
                        );
                        GpuiMenuItem::action(command.title.clone(), RunCommand(invocation))
                    });
                    items.push(GpuiMenuItem::submenu(
                        gpui_kit::Menu::new(name.clone()).items(entries),
                    ));
                }
            }
            MenuItem::Placeholder(Placeholder::RunCommands) => {
                for name in state.run_commands {
                    let invocation = Invocation::with_args("run.run", json!({ "name": name }));
                    items.push(GpuiMenuItem::action(name.clone(), RunCommand(invocation)));
                }
            }
            MenuItem::Placeholder(Placeholder::Documents) => {
                for (index, name) in state.documents.iter().enumerate() {
                    let invocation =
                        Invocation::with_args("view.go-to-tab", json!({ "index": index + 1 }));
                    items.push(
                        GpuiMenuItem::action(
                            format!("{}: {name}", index + 1),
                            RunCommand(invocation),
                        )
                        .checked(state.active_document == Some(index)),
                    );
                }
            }
            MenuItem::Placeholder(Placeholder::Macros) => {
                for name in state.macros {
                    let invocation = Invocation::with_args("macro.run", json!({ "name": name }));
                    items.push(GpuiMenuItem::action(name.clone(), RunCommand(invocation)));
                }
            }
            MenuItem::Placeholder(Placeholder::Themes) => {
                for name in state.themes {
                    let invocation =
                        Invocation::with_args("settings.theme", json!({ "name": name }));
                    let checked = (state.checked)(&invocation);
                    items.push(
                        GpuiMenuItem::action(name.clone(), RunCommand(invocation)).checked(checked),
                    );
                }
            }
        }
    }
    gpui_kit::Menu::new(crate::i18n::get_or(&menu_key(&menu.title), &menu.title)).items(items)
}

/// On macOS, About, Check for Updates and Quit live in the application menu.
pub fn mac_layout(model: Vec<Menu>) -> Vec<Menu> {
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
pub fn install(state: &MenuState, cx: &mut App) {
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
