//! A buffer: one open document (a file or an untitled "new N" tab) shared by the views showing it.

use std::path::{Path, PathBuf};

use birchpad_core::{Document, Selection, Transaction, UndoGrouping};
use gpui_kit::{Context, EntityId, EventEmitter};

/// What changed in a buffer, so that every view showing it can follow.
#[derive(Debug, Clone)]
pub(crate) enum BufferEvent {
    /// The text changed. `origin` is the view that made the change; it has already updated its
    /// own selection, the other views map theirs through `transaction`.
    Edited {
        transaction: Transaction,
        origin: Option<EntityId>,
    },
    /// The modified flag, path or format changed: titles and tabs need redrawing.
    StateChanged,
}

pub(crate) struct Buffer {
    doc: Document,
    path: Option<PathBuf>,
    /// The number of an untitled buffer: 1 for "new 1".
    untitled: Option<usize>,
}

impl EventEmitter<BufferEvent> for Buffer {}

impl Buffer {
    pub(crate) fn untitled(number: usize) -> Self {
        Self {
            doc: Document::new(),
            path: None,
            untitled: Some(number),
        }
    }

    pub(crate) fn from_file(doc: Document, path: PathBuf) -> Self {
        Self {
            doc,
            path: Some(path),
            untitled: None,
        }
    }

    pub(crate) fn doc(&self) -> &Document {
        &self.doc
    }

    pub(crate) fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub(crate) fn untitled_number(&self) -> Option<usize> {
        self.untitled
    }

    /// The name shown on the tab: the file name, or "new N".
    pub(crate) fn display_name(&self) -> String {
        match (&self.path, self.untitled) {
            (Some(path), _) => path.file_name().map_or_else(
                || path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            ),
            (None, Some(number)) => format!("new {number}"),
            (None, None) => "new".to_owned(),
        }
    }

    pub(crate) fn is_modified(&self) -> bool {
        self.doc.is_modified()
    }

    pub(crate) fn apply(
        &mut self,
        transaction: Transaction,
        selection_before: &Selection,
        grouping: UndoGrouping,
        origin: Option<EntityId>,
        cx: &mut Context<Self>,
    ) {
        let was_modified = self.doc.is_modified();
        self.doc.apply(&transaction, selection_before, grouping);
        cx.emit(BufferEvent::Edited {
            transaction,
            origin,
        });
        if was_modified != self.doc.is_modified() {
            cx.emit(BufferEvent::StateChanged);
        }
        cx.notify();
    }

    pub(crate) fn undo(
        &mut self,
        origin: Option<EntityId>,
        cx: &mut Context<Self>,
    ) -> Option<Transaction> {
        let transaction = self.doc.undo()?;
        self.after_history_step(transaction.clone(), origin, cx);
        Some(transaction)
    }

    pub(crate) fn redo(
        &mut self,
        origin: Option<EntityId>,
        cx: &mut Context<Self>,
    ) -> Option<Transaction> {
        let transaction = self.doc.redo()?;
        self.after_history_step(transaction.clone(), origin, cx);
        Some(transaction)
    }

    fn after_history_step(
        &mut self,
        transaction: Transaction,
        origin: Option<EntityId>,
        cx: &mut Context<Self>,
    ) {
        cx.emit(BufferEvent::Edited {
            transaction,
            origin,
        });
        cx.emit(BufferEvent::StateChanged);
        cx.notify();
    }
}
