//! Navigation history: Go Back and Go Forward through the places the caret jumped from.
//!
//! The workspace watches the caret of the active view. A move to another document, or ten
//! lines or more away, is a jump: the place it left is pushed on the back stack, and the
//! forward stack is dropped, as in a web browser. Smaller moves, typing included, only update
//! the current place, so Go Back returns to where the caret last was before the jump, not to
//! every line it passed. Places follow edits of their documents. A place in a document that
//! was closed opens its file again; one in a closed untitled document is dropped.

use std::path::PathBuf;

use birchpad_cli::CaretTarget;
use birchpad_core::motion::line_of;
use birchpad_core::{Assoc, Transaction};
use gpui_kit::{App, Context, Entity, WeakEntity, Window};

use crate::buffer::{Buffer, ReadOnly};
use crate::commands::CommandRegistry;
use crate::editor::EditorView;
use crate::workspace::Workspace;

/// Lines the caret must move to make a jump.
const JUMP_LINES: usize = 10;
/// Places kept on each stack.
const LIMIT: usize = 50;

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("navigate.back", |this, (), window, cx| {
        this.navigate(true, window, cx);
        Ok(())
    });
    registry.workspace("navigate.forward", |this, (), window, cx| {
        this.navigate(false, window, cx);
        Ok(())
    });
}

/// Back and forward stacks of places, and the current place.
#[derive(Debug, Clone)]
pub(crate) struct History<L> {
    back: Vec<L>,
    forward: Vec<L>,
    current: Option<L>,
}

impl<L> Default for History<L> {
    fn default() -> Self {
        Self {
            back: Vec::new(),
            forward: Vec::new(),
            current: None,
        }
    }
}

impl<L: Clone> History<L> {
    /// The caret is at `here`. If `jump` says it is far from the current place, that place is
    /// pushed on the back stack and the forward stack dropped.
    pub(crate) fn visit(&mut self, here: L, jump: impl FnOnce(&L, &L) -> bool) {
        if let Some(current) = self.current.take()
            && jump(&current, &here)
        {
            push(&mut self.back, current);
            self.forward.clear();
        }
        self.current = Some(here);
    }

    /// Go Back: the place to go to, which becomes the current one.
    pub(crate) fn back(&mut self) -> Option<L> {
        let target = self.back.pop()?;
        if let Some(current) = self.current.replace(target.clone()) {
            push(&mut self.forward, current);
        }
        Some(target)
    }

    /// Go Forward: the place to go to, which becomes the current one.
    pub(crate) fn forward(&mut self) -> Option<L> {
        let target = self.forward.pop()?;
        if let Some(current) = self.current.replace(target.clone()) {
            push(&mut self.back, current);
        }
        Some(target)
    }

    #[cfg(test)]
    pub(crate) fn can_go(&self, back: bool) -> bool {
        if back {
            !self.back.is_empty()
        } else {
            !self.forward.is_empty()
        }
    }

    /// The places of the stacks.
    fn places_mut(&mut self) -> impl Iterator<Item = &mut L> {
        self.back.iter_mut().chain(self.forward.iter_mut())
    }

    /// Drops the places of the stacks that `keep` refuses.
    fn retain(&mut self, keep: impl Fn(&L) -> bool) {
        self.back.retain(&keep);
        self.forward.retain(&keep);
    }
}

fn push<L>(stack: &mut Vec<L>, place: L) {
    stack.push(place);
    if stack.len() > LIMIT {
        stack.remove(0);
    }
}

/// A place: a document, its file if it has one, and a byte offset.
#[derive(Debug, Clone)]
pub(crate) struct Place {
    buffer: WeakEntity<Buffer>,
    path: Option<PathBuf>,
    pos: usize,
}

impl Place {
    /// Whether Go Back or Forward can still go there.
    fn reachable(&self) -> bool {
        self.buffer.upgrade().is_some() || self.path.is_some()
    }
}

impl Workspace {
    /// Notes where the caret of the active view is, unless its file is still being read.
    pub(crate) fn note_place(&mut self, cx: &App) {
        let Some(view) = self.active_view(cx) else {
            return;
        };
        let view = view.read(cx);
        let buffer = view.buffer.read(cx);
        if buffer.read_only() == Some(ReadOnly::Loading) {
            return;
        }
        let here = Place {
            buffer: view.buffer.downgrade(),
            path: buffer.path().map(Into::into),
            pos: view.selection.primary().head,
        };
        let text = buffer.doc().text();
        self.navigation.visit(here, |current, here| {
            current.buffer != here.buffer
                || line_of(text, current.pos.min(text.len()))
                    .abs_diff(line_of(text, here.pos.min(text.len())))
                    >= JUMP_LINES
        });
    }

    /// Moves the places in `buffer` along with an edit. The current place moves past text
    /// inserted where it is, as the caret does, so that typing is never taken for a jump.
    pub(crate) fn map_places(&mut self, buffer: &Entity<Buffer>, transaction: &Transaction) {
        let buffer = buffer.downgrade();
        let changes = transaction.changes();
        for place in self.navigation.places_mut() {
            if place.buffer == buffer {
                place.pos = changes.map_pos(place.pos, Assoc::Before);
            }
        }
        if let Some(current) = self.navigation.current_mut()
            && current.buffer == buffer
        {
            current.pos = changes.map_pos(current.pos, Assoc::After);
        }
    }

    /// Go Back (`back`) or Go Forward.
    fn navigate(&mut self, back: bool, window: &mut Window, cx: &mut Context<Self>) {
        // Where the caret is now is where Go Forward returns to.
        self.note_place(cx);
        self.navigation.retain(Place::reachable);
        let target = if back {
            self.navigation.back()
        } else {
            self.navigation.forward()
        };
        let Some(target) = target else {
            return;
        };
        let view = match target.buffer.upgrade() {
            Some(buffer) => self.view_of(&buffer, cx),
            None => None,
        };
        let view = match (view, &target.path) {
            (Some(view), _) => {
                self.activate_view(&view, window, cx);
                view.update(cx, |view, cx| {
                    let pos = view
                        .text(cx)
                        .floor_char_boundary(target.pos.min(view.text(cx).len()));
                    view.go_to(pos, cx);
                });
                view
            }
            (None, Some(path)) => {
                self.open_path(path, window, cx);
                let Some(view) = self.view_for_path(path, cx) else {
                    return;
                };
                view.update(cx, |view, cx| {
                    view.set_caret_target(CaretTarget::Position(target.pos), cx);
                });
                // The document is new: the history should point at it from now on.
                let buffer = view.read(cx).buffer.downgrade();
                if let Some(current) = self.navigation.current_mut() {
                    current.buffer = buffer;
                }
                view
            }
            (None, None) => return,
        };
        let focus = view.read(cx).focus_handle.clone();
        window.focus(&focus, cx);
        cx.notify();
    }

    /// A view of `buffer`: the active one if it shows it.
    fn view_of(&self, buffer: &Entity<Buffer>, cx: &App) -> Option<Entity<EditorView>> {
        self.active_view(cx)
            .filter(|view| &view.read(cx).buffer == buffer)
            .or_else(|| {
                self.all_views(cx)
                    .into_iter()
                    .find(|view| &view.read(cx).buffer == buffer)
            })
    }
}

impl<L> History<L> {
    fn current_mut(&mut self) -> Option<&mut L> {
        self.current.as_mut()
    }
}

#[cfg(test)]
#[path = "navigation_tests.rs"]
mod tests;
