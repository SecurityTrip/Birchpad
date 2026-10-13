//! What the parts of Birchpad's window share, so that each (the editor, the dialogs, the
//! workspace) needs only this and not the application: the command registry, the application's
//! settings and state, dialog buttons and messages to the user.

mod app_state;
mod commands;
mod report;

pub use app_state::{AppState, SessionPaths};
pub use commands::{CommandRegistry, CommandTarget, Handler, RunCommand};
#[cfg(feature = "test-support")]
pub use report::take_info;
pub use report::{
    dialog_action, dialog_close, report_error, report_info, report_success, report_warning,
};
