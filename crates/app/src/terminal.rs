//! View > Terminal (ADR 0067): the panel of `birchpad-terminal-panel` shown and hidden, its
//! shell started the first time it shows, in the active document's folder.

use std::path::PathBuf;

pub use birchpad_terminal_panel::*;
use gpui_kit::{App, Context, Window};

use crate::commands::{CommandRegistry, Register as _};
use crate::workspace::Workspace;

/// The folder a new shell starts in: the active document's, else the home folder.
fn start_folder(workspace: &Workspace, cx: &App) -> PathBuf {
    workspace
        .active_view(cx)
        .and_then(|view| {
            view.read(cx)
                .buffer
                .read(cx)
                .path()
                .and_then(|path| path.parent())
                .map(std::path::Path::to_owned)
        })
        .filter(|folder| folder.is_dir())
        .or_else(std::env::home_dir)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

impl Workspace {
    /// View > Terminal: shows the panel (starting a shell the first time) and gives it the
    /// focus, or hides it and gives the focus back to the document.
    pub fn toggle_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let terminal = self.terminal.clone();
        if terminal.read(cx).visible {
            terminal.update(cx, |terminal, cx| {
                terminal.visible = false;
                cx.notify();
            });
            if let Some(view) = self.active_view(cx) {
                let focus = view.read(cx).focus_handle.clone();
                window.focus(&focus, cx);
            }
        } else {
            let folder = start_folder(self, cx);
            terminal.update(cx, |terminal, cx| {
                terminal.visible = true;
                if terminal.session().is_none() {
                    // What went wrong shows in the panel.
                    let _ = terminal.start(folder, cx);
                }
                cx.notify();
            });
            let focus = terminal.read(cx).focus_handle.clone();
            window.focus(&focus, cx);
        }
        self.refresh_menus(cx);
        cx.notify();
    }
}

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("view.terminal", |this, (), window, cx| {
        this.toggle_terminal(window, cx);
        Ok(())
    });
}
