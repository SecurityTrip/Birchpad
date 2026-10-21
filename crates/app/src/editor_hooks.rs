//! The application's part in the editor (`birchpad_editor::EditorHooks`): macro recording,
//! links opened in the browser, the right-click menu, user defined languages and spelling.

use birchpad_editor::{EditorHooks, EditorView};
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::{App, Context, WeakEntity, Window};

/// Gives the editor the application's hooks.
pub fn install(cx: &mut App) {
    cx.set_global(EditorHooks {
        ran: crate::macros::record,
        typed: crate::macros::record_typing,
        open_link,
        context_menu,
        user_language_for_file: crate::user_languages::for_file,
        user_language_style: crate::user_languages::style_of,
        misspelled: crate::spell_check::underlined,
    });
}

fn open_link(url: String, cx: &mut App) {
    let _ = crate::file_commands::shell(crate::file_commands::Shell::Browse(url), cx);
}

/// The editor's right-click menu: `contextMenu.toml`, or the default, with what is wrong with
/// the file on top, and the commands plugins add to it at the bottom.
fn context_menu(
    menu: PopupMenu,
    view: &WeakEntity<EditorView>,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let (mut nodes, problem) = crate::context_menu::load(cx);
    nodes.extend(crate::context_menu::plugin_items(cx));
    let menu = match problem {
        Some(problem) => menu
            .item(PopupMenuItem::new(problem).disabled(true))
            .separator(),
        None => menu,
    };
    crate::context_menu::build(menu, &nodes, view, window, cx)
}
