//! Plugins and scripts in the window (ADR 0036, 0037): what they see of the active document,
//! and what Birchpad does with what they return.
//!
//! A call gets a copy of the active document ([`document_state`]); its edit is applied as one
//! undo step, then its effects run in order: commands, messages, the plugin panel. Commands a
//! plugin runs may run plugins in turn, up to [`MAX_DEPTH`] deep.

pub mod admin;
pub mod plugins;
pub mod scripts;

use std::cell::Cell;

use anyhow::{Result, bail};
use birchpad_plugin::{DocumentState, Effect, Outcome, Session};
use gpui_kit::{App, Context, Entity, Window};

use crate::editor::EditorView;
use crate::panels::{PanelKind, PluginPanel};
use crate::workspace::Workspace;

/// How deep plugins and scripts may run each other through commands.
pub const MAX_DEPTH: usize = 8;

thread_local! {
    static DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// Who filled the plugin panel, and is told when a line of it is clicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelOwner {
    Script(String),
    /// A plugin, by id.
    Plugin(String),
}

/// A copy of `view`'s document for a call.
pub fn document_state(view: &EditorView, cx: &App) -> DocumentState {
    let buffer = view.buffer.read(cx);
    let language = match (buffer.language(), buffer.user_language()) {
        (Some(language), _) => language.id.to_owned(),
        (None, Some(found)) => format!("{}{}", crate::user_languages::ID_PREFIX, found.name),
        (None, None) => "text".to_owned(),
    };
    DocumentState {
        text: buffer.doc().text().clone(),
        selection: view.selection.clone(),
        path: buffer.path().map(|path| path.display().to_string()),
        language,
    }
}

/// A session over the active document, or without one.
pub fn session(workspace: &Workspace, cx: &App) -> (Option<Entity<EditorView>>, Session) {
    match workspace.active_view(cx) {
        Some(view) => {
            let state = document_state(view.read(cx), cx);
            (Some(view), Session::new(state))
        }
        None => (None, Session::empty()),
    }
}

/// Runs `call` unless plugins and scripts are already running each other too deep.
pub fn nested<T>(call: impl FnOnce() -> Result<T>) -> Result<T> {
    let depth = DEPTH.with(Cell::get);
    if depth >= MAX_DEPTH {
        bail!(
            "{}",
            t!(
                "extensions.plugins-and-scripts-run-each-other-more-than-dee",
                MAX_DEPTH = MAX_DEPTH
            )
        );
    }
    DEPTH.with(|cell| cell.set(depth + 1));
    let result = call();
    DEPTH.with(|cell| cell.set(depth));
    result
}

impl Workspace {
    /// Applies what a call returned: its edit to `view`, then its effects. The first command
    /// that fails is the error; the others still run.
    pub fn apply_outcome(
        &mut self,
        view: Option<&Entity<EditorView>>,
        owner: PanelOwner,
        outcome: Outcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        if let Some(transaction) = outcome.transaction {
            let Some(view) = view else {
                bail!("{}", t!("extensions.there-is-no-document-to-change"));
            };
            if !view.update(cx, |view, cx| view.apply_command_edit(transaction, cx)) {
                bail!("{}", t!("extensions.the-document-cannot-be-changed"));
            }
        }
        let mut failed = None;
        for effect in outcome.effects {
            match effect {
                Effect::RunCommand(invocation) => {
                    let result = nested(|| self.dispatch(&invocation, window, cx));
                    if let Err(error) = result {
                        failed.get_or_insert(error);
                    }
                }
                Effect::Message(text) => crate::workspace::report_info(text, window, cx),
                Effect::Panel { title, lines } => {
                    self.show_plugin_panel(owner.clone(), title, lines, window, cx);
                }
            }
        }
        failed.map_or(Ok(()), Err)
    }

    /// Fills the plugin panel and opens it.
    pub fn show_plugin_panel(
        &mut self,
        owner: PanelOwner,
        title: String,
        lines: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_panel(PanelKind::PluginPanel, window, cx);
        if let Some(panel) = self
            .docks
            .view(PanelKind::PluginPanel)
            .and_then(|view| view.clone().downcast::<PluginPanel>().ok())
        {
            panel.update(cx, |panel, cx| panel.fill(owner, title, lines, cx));
        }
    }

    /// A click on line `line` of the plugin panel.
    pub fn plugin_panel_clicked(
        &mut self,
        owner: &PanelOwner,
        line: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        match owner {
            // A script's panel only shows.
            PanelOwner::Script(_) => Ok(()),
            PanelOwner::Plugin(id) => self.plugin_panel_click(id, line, window, cx),
        }
    }
}
