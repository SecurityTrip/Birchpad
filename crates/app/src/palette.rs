//! View > Command Palette...: every command by its name, picked by typing a few of its
//! letters. It lists the commands of the catalog that run as they are, the menu items that run
//! one with arguments (Language: Python), the saved macros and command lines and the plugins'
//! and scripts' commands, each with its shortcut. The commands last run from the palette come
//! first, as many as Preferences > Command Palette says; they are kept in `state.toml`.
//! Commands that are turned off or not implemented yet are not listed.

use std::rc::Rc;

use birchpad_commands::{Invocation, Keymap, Platform};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::{App, Window};

use crate::app_state::AppState;
use crate::commands::{ActiveKeymap, CommandRegistry, Register as _};
use crate::quick_pick::{self, PickItem};
use crate::shortcut_mapper::{self, MapperTab};
use crate::workspace::Workspace;

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("view.command-palette", |_, (), window, cx| {
        open(cx.entity().downgrade(), window, cx);
        Ok(())
    });
}

/// A command the palette lists.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub invocation: Invocation,
    pub title: String,
    /// Its shortcut, as the menus show it; empty if it has none.
    pub shortcut: String,
}

/// The key a command is remembered by in `state.toml`: its invocation as JSON.
pub fn recent_key(invocation: &Invocation) -> String {
    serde_json::to_string(invocation).unwrap_or_else(|_| invocation.command.clone())
}

/// The shortcut of `invocation` in `keymap` as the menus show it (`Ctrl+Shift+A`): the last
/// binding of this platform, as menus take the last one.
pub fn shortcut(keymap: &Keymap, invocation: &Invocation) -> String {
    let platform = Platform::current().name();
    keymap
        .bindings_for(invocation)
        .filter(|binding| {
            binding
                .platforms
                .as_ref()
                .is_none_or(|platforms| platforms.iter().any(|name| name == platform))
        })
        .last()
        .map(|binding| {
            binding
                .keys
                .iter()
                .map(|keys| {
                    let text = keys.to_string();
                    gpui_kit::Keystroke::parse(&text)
                        .map_or(text, |keystroke| Kbd::format(&keystroke))
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

/// Every command the palette lists, in the menus' order: those of the catalog, the menu items
/// with arguments, the macros, command lines, plugins' and scripts' commands; each once, the
/// recently used first (`recent`, keys from [`recent_key`], most recent first). Returns the
/// entries and how many of them are recent.
pub fn entries(recent: &[String], cx: &App) -> (Vec<Entry>, usize) {
    let keymap = &cx.global::<ActiveKeymap>().0;
    let registry = cx.global::<CommandRegistry>();
    let runs = |invocation: &Invocation| {
        invocation.command != "view.command-palette"
            && registry.contains(&invocation.command)
            && registry.is_enabled(&invocation.command, cx)
            && (!invocation.args.is_null() || registry.runs_bare(&invocation.command))
    };
    let catalog = birchpad_commands::COMMANDS
        .iter()
        .map(|spec| shortcut_mapper::Row {
            invocation: Invocation::new(spec.id),
            name: crate::menus::command_title(spec.id),
            category: String::new(),
        });
    let others = [MapperTab::Macros, MapperTab::Run, MapperTab::Plugins]
        .into_iter()
        .flat_map(|tab| shortcut_mapper::rows_of(tab, keymap, cx));
    let mut all: Vec<Entry> = Vec::new();
    for row in catalog.chain(shortcut_mapper::menu_rows()).chain(others) {
        if runs(&row.invocation) && !all.iter().any(|entry| entry.invocation == row.invocation) {
            all.push(Entry {
                shortcut: shortcut(keymap, &row.invocation),
                invocation: row.invocation,
                title: row.name,
            });
        }
    }
    let mut first = Vec::new();
    for key in recent {
        let Ok(invocation) = serde_json::from_str::<Invocation>(key) else {
            continue;
        };
        if let Some(at) = all.iter().position(|entry| entry.invocation == invocation) {
            first.push(all.remove(at));
        }
    }
    let count = first.len();
    first.extend(all);
    (first, count)
}

/// Remembers `invocation` as the command run last from the palette.
pub fn remember(invocation: &Invocation, cx: &mut App) {
    let limit = AppState::global(cx).settings.palette.recent_limit();
    let key = recent_key(invocation);
    AppState::update_state(cx, |state, _| state.add_recent_command(&key, limit));
}

/// Opens the palette.
pub fn open(workspace: gpui_kit::WeakEntity<Workspace>, window: &mut Window, cx: &mut App) {
    let app = AppState::global(cx);
    let limit = app.settings.palette.recent_limit();
    let recent: Vec<String> = app
        .state
        .recent_commands
        .iter()
        .take(limit)
        .cloned()
        .collect();
    let (entries, recent) = entries(&recent, cx);
    let items = entries
        .iter()
        .map(|entry| PickItem::new(entry.title.clone(), entry.shortcut.clone()))
        .collect();
    let entries = Rc::new(entries);
    quick_pick::open(
        t!("palette.command-palette"),
        t!("palette.type-a-command"),
        t!("palette.no-command-matches"),
        items,
        recent,
        Rc::new(move |index, window, cx| {
            let invocation = entries[index].invocation.clone();
            remember(&invocation, cx);
            let result = workspace
                .update(cx, |workspace, cx| {
                    workspace.dispatch(&invocation, window, cx)
                })
                .unwrap_or(Ok(()));
            if let Err(error) = result {
                crate::workspace::report_error(&error, window, cx);
            }
        }),
        window,
        cx,
    );
}
