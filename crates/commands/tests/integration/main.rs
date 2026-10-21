//! The crate's tests through its public interface, one module per part of `src`, in one
//! test binary.

#[path = "catalog/tests.rs"]
mod catalog;

#[path = "context_menu/tests.rs"]
mod context_menu;

#[path = "invocation/tests.rs"]
mod invocation;

#[path = "keymap/tests.rs"]
mod keymap;

#[path = "keymap_edit/tests.rs"]
mod keymap_edit;

#[path = "keystroke/properties.rs"]
mod keystroke_properties;

#[path = "keystroke/tests.rs"]
mod keystroke;

#[path = "macros/tests.rs"]
mod macros;

#[path = "menu/tests.rs"]
mod menu;

#[path = "run/tests.rs"]
mod run;
