//! Starting Birchpad: the command line, compatible with Notepad++'s options, and handing it to
//! an already running instance. No UI dependencies.

mod args;
mod instance;

pub use args::{CaretTarget, CommandLine};
pub use instance::{Address, Instance, Server, hand_off, start};
