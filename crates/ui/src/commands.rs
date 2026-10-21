//! The command registry: maps command ids from `birchpad-commands` to handlers.
//!
//! GPUI sees a single action, [`RunCommand`], carrying an [`Invocation`]. Key bindings and menu
//! items are built from the data in `birchpad-commands` and all dispatch that action; the view
//! that handles it looks the id up here and runs the handler. Nothing else invokes command code
//! directly.
//!
//! A handler is registered for a kind of target, a [`CommandTarget`]: the view of a scope of the
//! catalog (the workspace, the editor) that runs it. The registry does not know those views; it
//! hands each the handlers registered for its own type.

use std::any::{Any, TypeId};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use anyhow::Result;
use birchpad_commands::{Invocation, Scope};
use gpui_kit::{Action, App, Context, Global, Window};
use serde::de::DeserializeOwned;

/// The one GPUI action of Birchpad: run a command from the catalog.
#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = birchpad, no_json)]
pub struct RunCommand(pub Invocation);

impl RunCommand {
    pub fn new(id: &str) -> Self {
        Self(Invocation::new(id))
    }
}

/// A view that runs the commands of a scope of the catalog.
pub trait CommandTarget: 'static {
    const SCOPE: Scope;
}

/// A handler, with the invocation's arguments still to decode.
pub type Handler<T> = dyn Fn(&mut T, &Invocation, &mut Window, &mut Context<T>) -> Result<()>;

struct Entry {
    target: TypeId,
    /// An `Rc<Handler<T>>` for the target `T`.
    handler: Rc<dyn Any>,
}

#[derive(Default)]
pub struct CommandRegistry {
    handlers: HashMap<&'static str, Entry>,
    /// Commands in the catalog that are not implemented yet: shown disabled in menus.
    pending: HashSet<&'static str>,
    /// Commands that are available only in some states (e.g. turned off by a setting).
    enabled: HashMap<&'static str, fn(&App) -> bool>,
}

impl Global for CommandRegistry {}

impl CommandRegistry {
    /// Registers a command of `T`'s scope whose arguments decode to `A` (`()` for none).
    ///
    /// # Panics
    ///
    /// If the command is not in the catalog, is of another scope, or is registered already.
    pub fn register<T: CommandTarget, A: DeserializeOwned + 'static>(
        &mut self,
        id: &'static str,
        run: impl Fn(&mut T, A, &mut Window, &mut Context<T>) -> Result<()> + 'static,
    ) {
        let spec = birchpad_commands::find(id)
            .unwrap_or_else(|| panic!("command {id} is not in the catalog"));
        assert_eq!(
            spec.scope,
            T::SCOPE,
            "command {id} is registered in the wrong scope"
        );
        let handler: Rc<Handler<T>> = Rc::new(
            move |this: &mut T,
                  invocation: &Invocation,
                  window: &mut Window,
                  cx: &mut Context<T>| { run(this, invocation.args()?, window, cx) },
        );
        let entry = Entry {
            target: TypeId::of::<T>(),
            handler: Rc::new(handler),
        };
        let previous = self.handlers.insert(id, entry);
        assert!(previous.is_none(), "command {id} is registered twice");
    }

    /// The handler of `id` if `T` runs it.
    pub fn handler<T: CommandTarget>(&self, id: &str) -> Option<Rc<Handler<T>>> {
        let entry = self.handlers.get(id)?;
        if entry.target != TypeId::of::<T>() {
            return None;
        }
        entry.handler.downcast_ref::<Rc<Handler<T>>>().cloned()
    }

    /// Whether `id` has a handler.
    pub fn contains(&self, id: &str) -> bool {
        self.handlers.contains_key(id)
    }

    /// Marks a catalog command as not implemented yet.
    ///
    /// # Panics
    ///
    /// If the command is not in the catalog or has a handler.
    pub fn pending(&mut self, id: &'static str) {
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

    pub fn is_pending(&self, id: &str) -> bool {
        self.pending.contains(id)
    }

    /// Makes a registered command available only while `enabled` says so; menus show it
    /// disabled otherwise.
    ///
    /// # Panics
    ///
    /// If the command has no handler.
    pub fn enabled_when(&mut self, id: &'static str, enabled: fn(&App) -> bool) {
        assert!(
            self.handlers.contains_key(id),
            "command {id} is not registered"
        );
        self.enabled.insert(id, enabled);
    }

    /// Whether the command can run now.
    pub fn is_enabled(&self, id: &str, cx: &App) -> bool {
        !self.is_pending(id) && self.enabled.get(id).is_none_or(|enabled| enabled(cx))
    }
}
