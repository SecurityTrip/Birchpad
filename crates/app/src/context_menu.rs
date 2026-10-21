//! The text's context menu, as Notepad++'s: a right click in the text opens it (moving the
//! caret there unless the click is in a selection), and its items run their commands on the
//! document. Which items it has is `context-menu.toml` next to the settings, the counterpart of
//! Notepad++'s `contextMenu.xml` (its format: `birchpad_commands::context_menu`); Settings >
//! Edit Popup ContextMenu opens that file, writing the default menu into it first. Changes
//! apply the next time the menu opens.

use std::path::PathBuf;

use crate::app_state::AppState;
use crate::commands::{CommandRegistry, Register as _};
pub use crate::editor::{EditorEvent, EditorView};
use crate::workspace::Workspace;
use anyhow::anyhow;
use birchpad_commands::Invocation;
pub use birchpad_commands::context_menu::{DEFAULT, FILE_NAME, Node, parse};
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::{App, ClickEvent, Context, WeakEntity, Window};

/// The user's menu file, if there is a settings folder.
pub fn file_path(cx: &App) -> Option<PathBuf> {
    AppState::global(cx)
        .paths
        .user_config_dir()
        .map(|dir| dir.join(FILE_NAME))
}

/// The menu to show: the user's file, or the default one. A file that cannot be read gives the
/// default menu and the reason.
pub fn load(cx: &App) -> (Vec<Node>, Option<String>) {
    let default = || parse(DEFAULT).expect("the default context menu reads");
    let Some(path) = file_path(cx) else {
        return (default(), None);
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return (default(), None),
        Err(error) => return (default(), Some(format!("{}: {error}", path.display()))),
    };
    match parse(&text) {
        Ok(nodes) => (nodes, None),
        Err(error) => (default(), Some(format!("{}: {error:#}", path.display()))),
    }
}

/// The items plugins add below the menu (ADR 0046): a separator, then each command a ready
/// plugin marks for the context menu, by its title; none if no plugin adds any.
pub fn plugin_items(cx: &App) -> Vec<Node> {
    let commands = crate::extensions::plugins::context_menu_commands(cx);
    if commands.is_empty() {
        return Vec::new();
    }
    std::iter::once(Node::Separator)
        .chain(commands.into_iter().map(|(plugin, command)| Node::Command {
            invocation: Invocation::with_args(
                "plugin.run",
                serde_json::json!({ "plugin": plugin, "command": command.id }),
            ),
            label: Some(command.title),
        }))
        .collect()
}

/// Fills `menu` with `nodes`; a chosen item asks `view` to run its command.
pub fn build(
    mut menu: PopupMenu,
    nodes: &[Node],
    view: &WeakEntity<EditorView>,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    for node in nodes {
        menu = match node {
            Node::Separator => menu.separator(),
            Node::Command { invocation, label } => {
                let text = match label {
                    Some(label) => crate::i18n::get_or(&crate::menus::menu_key(label), label),
                    None => crate::menus::command_title(&invocation.command),
                };
                let enabled = cx
                    .global::<CommandRegistry>()
                    .is_enabled(&invocation.command, cx);
                let (invocation, view) = (invocation.clone(), view.clone());
                menu.item(PopupMenuItem::new(text).disabled(!enabled).on_click(
                    move |_: &ClickEvent, _, cx| {
                        let invocation = invocation.clone();
                        view.update(cx, |_, cx| cx.emit(EditorEvent::Command(invocation)))
                            .ok();
                    },
                ))
            }
            Node::Folder { name, items } => {
                let label = crate::i18n::get_or(&crate::menus::menu_key(name), name);
                let (items, view) = (items.clone(), view.clone());
                menu.submenu(label, window, cx, move |menu, window, cx| {
                    build(menu, &items, &view, window, cx)
                })
            }
        };
    }
    menu
}

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("settings.edit-context-menu", |this, (), window, cx| {
        let path = file_path(cx)
            .ok_or_else(|| anyhow!("{}", t!("context-menu.there-is-no-settings-folder")))?;
        if !path.exists() {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&path, DEFAULT)?;
        }
        this.open_path(&path, window, cx);
        Ok(())
    });
}

impl Workspace {
    /// A command chosen in a view's context menu: runs on that view's document.
    pub fn run_context_command(
        &mut self,
        view: &gpui_kit::Entity<EditorView>,
        invocation: &Invocation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.activate_view(view, window, cx);
        if let Err(error) = self.dispatch(invocation, window, cx) {
            crate::workspace::report_error(&error, window, cx);
        }
    }
}
