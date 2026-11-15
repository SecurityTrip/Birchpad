//! Tools > Language Server (ADR 0065) in the window: the documents of a workspace told to
//! the servers, and the commands that ask a server ([`commands`]). The servers themselves, kept
//! in step with the documents, are `birchpad-language-servers`'.

pub mod commands;

pub use birchpad_language_servers::*;
use gpui_kit::{AnyWindowHandle, Context};

use crate::workspace::Workspace;

/// Brings the servers in step with the documents of `workspace`.
pub fn sync_workspace(workspace: &Workspace, window: AnyWindowHandle, cx: &mut Context<Workspace>) {
    let documents: Vec<_> = workspace
        .all_views(cx)
        .into_iter()
        .map(|view| view.read(cx).buffer.clone())
        .collect();
    sync_documents(cx.entity_id(), documents, window, cx);
}
