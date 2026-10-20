//! Starting Birchpad: the command line, compatible with Notepad++'s options, handing it to an
//! already running instance, and the Windows Explorer's "Edit with Birchpad". No UI
//! dependencies.

mod args;
pub mod associations;
pub mod explorer;
mod instance;

pub use args::{CaretTarget, CommandLine};
pub use instance::{
    Address, Instance, MAX_MESSAGE, MAX_SOCKET_PATH, Server, hand_off, socket_path, start,
};
