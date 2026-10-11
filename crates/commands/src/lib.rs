//! Commands, key bindings and menus of Birchpad, as data.
//!
//! Every user-visible action has a string id (`file.open`, `edit.undo`, `view.zoom-in`) and
//! optional JSON arguments; together they form an [`Invocation`]. Key bindings, menus and, later,
//! macros, the command palette and plugins all refer to commands by invocation, never to code, so
//! they can be stored, edited and replayed. This crate knows nothing about windows or GPUI: the
//! application maps invocations to handlers and keymaps to its UI toolkit.

mod catalog;
mod invocation;
mod keymap;
mod keymap_edit;
mod keystroke;
mod macros;
mod menu;
mod run;

pub use catalog::{COMMANDS, CommandSpec, Scope, find};
pub use invocation::{ArgsError, Invocation};
pub use keymap::{Binding, DEFAULT_KEYMAP, Keymap, KeymapDiagnostic, KeymapError, Layer};
pub use keymap_edit::{bind, reset, unbind};
pub use keystroke::{Keystroke, KeystrokeError, Modifiers, Platform};
pub use macros::{Macro, is_recordable, parse_macros, record, write_macros};
pub use menu::{Menu, MenuItem, Placeholder, main_menu};
pub use run::{
    RunEntry, RunVariables, default_run_commands, expand_variables, parse_run_commands,
    write_run_commands,
};
