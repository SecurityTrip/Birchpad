//! The command registry: maps command ids from `birchpad-commands` to handlers.
//!
//! GPUI sees a single action, [`RunCommand`], carrying an [`Invocation`]. Key bindings and menu
//! items are built from the data in `birchpad-commands` and all dispatch that action; the focused
//! editor view or the workspace looks the id up here and runs the handler. Nothing else invokes
//! command code directly.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use anyhow::Result;
use birchpad_commands::{Invocation, Keymap, Layer, Platform, Scope};
use gpui_kit::{
    Action, App, Context, Global, KeyBinding, KeyBindingContextPredicate, PlatformKeyboardMapper,
    Window,
};
use serde::de::DeserializeOwned;

use crate::editor::EditorView;
use crate::workspace::Workspace;

/// The one GPUI action of Birchpad: run a command from the catalog.
#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = birchpad, no_json)]
pub(crate) struct RunCommand(pub(crate) Invocation);

impl RunCommand {
    pub(crate) fn new(id: &str) -> Self {
        Self(Invocation::new(id))
    }
}

type WorkspaceFn =
    dyn Fn(&mut Workspace, &Invocation, &mut Window, &mut Context<Workspace>) -> Result<()>;
type EditorFn =
    dyn Fn(&mut EditorView, &Invocation, &mut Window, &mut Context<EditorView>) -> Result<()>;

#[derive(Clone)]
pub(crate) enum Handler {
    Workspace(Rc<WorkspaceFn>),
    Editor(Rc<EditorFn>),
}

#[derive(Default)]
pub(crate) struct CommandRegistry {
    handlers: HashMap<&'static str, Handler>,
    /// Commands in the catalog that are not implemented yet: shown disabled in menus.
    pending: HashSet<&'static str>,
    /// Commands that are available only in some states (e.g. turned off by a setting).
    enabled: HashMap<&'static str, fn(&App) -> bool>,
}

impl Global for CommandRegistry {}

impl CommandRegistry {
    /// A registry with every handler of the application.
    pub(crate) fn new() -> Self {
        let mut registry = Self::default();
        crate::workspace::register_commands(&mut registry);
        crate::editor::register_commands(&mut registry);
        registry
    }

    /// Registers a workspace command whose arguments decode to `A` (`()` for none).
    pub(crate) fn workspace<A: DeserializeOwned + 'static>(
        &mut self,
        id: &'static str,
        run: impl Fn(&mut Workspace, A, &mut Window, &mut Context<Workspace>) -> Result<()> + 'static,
    ) {
        let handler = move |this: &mut Workspace,
                            invocation: &Invocation,
                            window: &mut Window,
                            cx: &mut Context<Workspace>| {
            run(this, invocation.args()?, window, cx)
        };
        self.insert(id, Scope::Workspace, Handler::Workspace(Rc::new(handler)));
    }

    /// Registers an editor command whose arguments decode to `A` (`()` for none).
    pub(crate) fn editor<A: DeserializeOwned + 'static>(
        &mut self,
        id: &'static str,
        run: impl Fn(&mut EditorView, A, &mut Window, &mut Context<EditorView>) -> Result<()> + 'static,
    ) {
        let handler = move |this: &mut EditorView,
                            invocation: &Invocation,
                            window: &mut Window,
                            cx: &mut Context<EditorView>| {
            run(this, invocation.args()?, window, cx)
        };
        self.insert(id, Scope::Editor, Handler::Editor(Rc::new(handler)));
    }

    /// Marks a catalog command as not implemented yet.
    #[expect(
        dead_code,
        reason = "every catalog command is implemented at the moment"
    )]
    pub(crate) fn pending(&mut self, id: &'static str) {
        assert!(
            birchpad_commands::find(id).is_some(),
            "command {id} is not in the catalog"
        );
        assert!(
            !self.handlers.contains_key(id),
            "command {id} is implemented"
        );
        self.pending.insert(id);
    }

    pub(crate) fn is_pending(&self, id: &str) -> bool {
        self.pending.contains(id)
    }

    /// Makes a registered command available only while `enabled` says so; menus show it
    /// disabled otherwise.
    pub(crate) fn enabled_when(&mut self, id: &'static str, enabled: fn(&App) -> bool) {
        assert!(
            self.handlers.contains_key(id),
            "command {id} is not registered"
        );
        self.enabled.insert(id, enabled);
    }

    /// Whether the command can run now.
    pub(crate) fn is_enabled(&self, id: &str, cx: &App) -> bool {
        !self.is_pending(id) && self.enabled.get(id).is_none_or(|enabled| enabled(cx))
    }

    fn insert(&mut self, id: &'static str, scope: Scope, handler: Handler) {
        let spec = birchpad_commands::find(id)
            .unwrap_or_else(|| panic!("command {id} is not in the catalog"));
        assert_eq!(
            spec.scope, scope,
            "command {id} is registered in the wrong scope"
        );
        let previous = self.handlers.insert(id, handler);
        assert!(previous.is_none(), "command {id} is registered twice");
    }

    pub(crate) fn get(&self, id: &str) -> Option<Handler> {
        self.handlers.get(id).cloned()
    }

    #[cfg(test)]
    fn contains(&self, id: &str) -> bool {
        self.handlers.contains_key(id)
    }
}

/// Installs the registry and the keymap (defaults plus the user's `keymap.toml`, if any).
pub(crate) fn init(user_keymap: Option<&str>, cx: &mut App) -> Keymap {
    cx.set_global(CommandRegistry::new());
    let mut keymap = Keymap::with_defaults(Platform::current());
    if let Some(source) = user_keymap {
        keymap.add_layer(Layer::User, source);
    }
    for diagnostic in keymap.diagnostics() {
        eprintln!("keymap: {diagnostic}");
    }
    let mapper = cx.keyboard_mapper().clone();
    cx.bind_keys(key_bindings(&keymap, mapper.as_ref()));
    keymap
}

/// Turns the effective bindings of a keymap into GPUI key bindings.
///
/// Keys go through the platform's keyboard mapper: Windows reports Shift with a digit or
/// punctuation key as the character it types on the current layout (Alt+Shift+0 arrives as
/// `alt-)` on a US or Russian layout), so `alt-shift-0` in a keymap has to become that too.
pub(crate) fn key_bindings(
    keymap: &Keymap,
    mapper: &dyn PlatformKeyboardMapper,
) -> Vec<KeyBinding> {
    keymap
        .bindings()
        .iter()
        .filter_map(|binding| {
            let context = match binding
                .context
                .as_deref()
                .map(KeyBindingContextPredicate::parse)
            {
                None => None,
                Some(Ok(predicate)) => Some(Rc::new(predicate)),
                Some(Err(error)) => {
                    eprintln!("keymap: bad context {:?}: {error}", binding.context);
                    return None;
                }
            };
            KeyBinding::load(
                &binding.keys_string(),
                Box::new(RunCommand(binding.invocation.clone())),
                context,
                false,
                None,
                mapper,
            )
            .inspect_err(|error| eprintln!("keymap: {}: {error}", binding.keys_string()))
            .ok()
        })
        .collect()
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
mod tests {
    use super::*;

    #[test]
    fn every_catalog_command_has_a_handler() {
        let registry = CommandRegistry::new();
        assert_eq!(unregistered(&registry), Vec::<&str>::new());
    }

    #[test]
    fn default_keymap_converts_to_gpui_bindings() {
        for platform in [Platform::Windows, Platform::Linux, Platform::MacOs] {
            let keymap = Keymap::with_defaults(platform);
            let mapper = gpui_kit::DummyKeyboardMapper;
            assert_eq!(
                key_bindings(&keymap, &mapper).len(),
                keymap.bindings().len()
            );
        }
    }
}
