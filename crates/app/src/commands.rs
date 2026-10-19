//! The application's commands: every handler registered in `birchpad_ui`'s command registry,
//! for the workspace or the editor, and the keymap that binds keys to them.

use anyhow::Result;
#[cfg(test)]
use birchpad_commands::{Invocation, Platform};
use birchpad_commands::{Keymap, Scope};
pub(crate) use birchpad_ui::{
    ActiveKeymap, CommandRegistry, CommandTarget, RunCommand, reload_keymap,
};
#[cfg(test)]
use birchpad_ui::{key_bindings, replace_command_bindings};
#[cfg(test)]
use gpui_kit::PlatformKeyboardMapper;
use gpui_kit::{App, Context, Window};
use serde::de::DeserializeOwned;

use crate::editor::EditorView;
use crate::workspace::Workspace;

impl CommandTarget for Workspace {
    const SCOPE: Scope = Scope::Workspace;
}

/// Registering the handlers of the workspace and of the editor.
pub(crate) trait Register {
    /// Registers a workspace command whose arguments decode to `A` (`()` for none).
    fn workspace<A: DeserializeOwned + 'static>(
        &mut self,
        id: &'static str,
        run: impl Fn(&mut Workspace, A, &mut Window, &mut Context<Workspace>) -> Result<()> + 'static,
    );

    /// Registers an editor command whose arguments decode to `A` (`()` for none).
    fn editor<A: DeserializeOwned + 'static>(
        &mut self,
        id: &'static str,
        run: impl Fn(&mut EditorView, A, &mut Window, &mut Context<EditorView>) -> Result<()> + 'static,
    );
}

impl Register for CommandRegistry {
    fn workspace<A: DeserializeOwned + 'static>(
        &mut self,
        id: &'static str,
        run: impl Fn(&mut Workspace, A, &mut Window, &mut Context<Workspace>) -> Result<()> + 'static,
    ) {
        self.register(id, run);
    }

    fn editor<A: DeserializeOwned + 'static>(
        &mut self,
        id: &'static str,
        run: impl Fn(&mut EditorView, A, &mut Window, &mut Context<EditorView>) -> Result<()> + 'static,
    ) {
        self.register(id, run);
    }
}

/// A registry with every handler of the application.
pub(crate) fn registry() -> CommandRegistry {
    let mut registry = CommandRegistry::default();
    crate::workspace::register_commands(&mut registry);
    crate::editor::register_commands(&mut registry);
    registry
}

/// Installs the registry and the keymap (defaults plus the user's `keymap.toml`, if any).
pub(crate) fn init(user_keymap: Option<&str>, cx: &mut App) -> Keymap {
    cx.set_global(registry());
    crate::editor_hooks::install(cx);
    let keymap = birchpad_ui::bind_keymap(user_keymap, cx);
    birchpad_menu_bar::init(cx);
    keymap
}

/// Catalog commands that have neither a handler nor a "pending" mark.
#[cfg(test)]
fn unregistered(registry: &CommandRegistry) -> Vec<&'static str> {
    birchpad_commands::COMMANDS
        .iter()
        .map(|spec| spec.id)
        .filter(|id| !registry.contains(id) && !registry.is_pending(id))
        .collect()
}

#[cfg(test)]
#[path = "../tests/unit/commands/tests.rs"]
mod tests;
