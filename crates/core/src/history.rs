//! Linear undo/redo history, as in Notepad++: making a new edit after undoing discards the
//! undone steps.

use ropey::Rope;

use crate::format::Format;
use crate::selection::Selection;
use crate::transaction::Transaction;

/// Identifies a state of the document's history.
///
/// Every new or merged undo step gets a fresh id, so comparing the current id with the id saved
/// to disk tells whether the document is modified, even after undo, redo and further edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RevisionId(u64);

impl RevisionId {
    /// The state before any edit.
    pub const INITIAL: Self = Self(0);
}

/// How a committed transaction relates to the previous undo step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoGrouping {
    /// Start a new undo step.
    NewStep,
    /// Fold into the previous undo step when possible, so that e.g. a typed word is undone at
    /// once. Falls back to a new step when there is no previous step or there is something to
    /// redo.
    MergeWithPrevious,
}

#[derive(Debug, Clone)]
struct Revision {
    id: RevisionId,
    forward: Transaction,
    inverse: Transaction,
}

#[derive(Debug, Clone)]
pub struct History {
    revisions: Vec<Revision>,
    /// Number of revisions currently applied; `revisions[applied..]` can be redone.
    applied: usize,
    next_id: u64,
}

impl Default for History {
    fn default() -> Self {
        Self {
            revisions: Vec::new(),
            applied: 0,
            next_id: 1,
        }
    }
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current(&self) -> RevisionId {
        match self.applied {
            0 => RevisionId::INITIAL,
            n => self.revisions[n - 1].id,
        }
    }

    pub fn can_undo(&self) -> bool {
        self.applied > 0
    }

    pub fn can_redo(&self) -> bool {
        self.applied < self.revisions.len()
    }

    /// Records `transaction`, which has just been applied to `original` in `original_format`.
    ///
    /// `selection_before` is restored when the step is undone, and so is the format if the
    /// transaction changed it.
    pub fn commit(
        &mut self,
        transaction: Transaction,
        original: &Rope,
        original_format: Format,
        selection_before: &Selection,
        grouping: UndoGrouping,
    ) {
        let inverse_changes = transaction.changes.invert(original);
        let inverse_format = transaction.format.map(|_| original_format);
        let id = self.fresh_id();

        if grouping == UndoGrouping::MergeWithPrevious && self.can_undo() && !self.can_redo() {
            let previous = self.revisions.pop().expect("can_undo implies a revision");
            let forward = Transaction {
                changes: previous.forward.changes.compose(transaction.changes),
                selection: transaction.selection.or(previous.forward.selection),
                format: transaction.format.or(previous.forward.format),
            };
            // Undoing the merged step undoes the new part first, then the previous one, so the
            // format to restore is the oldest one recorded.
            let inverse = Transaction {
                changes: inverse_changes.compose(previous.inverse.changes),
                selection: previous.inverse.selection,
                format: previous.inverse.format.or(inverse_format),
            };
            self.revisions.push(Revision {
                id,
                forward,
                inverse,
            });
            return;
        }

        self.revisions.truncate(self.applied);
        let mut inverse =
            Transaction::new(inverse_changes).with_selection(selection_before.clone());
        inverse.format = inverse_format;
        self.revisions.push(Revision {
            id,
            forward: transaction,
            inverse,
        });
        self.applied += 1;
    }

    /// Steps back and returns the transaction to apply to the text to undo the step.
    pub fn undo(&mut self) -> Option<&Transaction> {
        if !self.can_undo() {
            return None;
        }
        self.applied -= 1;
        Some(&self.revisions[self.applied].inverse)
    }

    /// Steps forward and returns the transaction to apply to the text to redo the step.
    pub fn redo(&mut self) -> Option<&Transaction> {
        if !self.can_redo() {
            return None;
        }
        self.applied += 1;
        Some(&self.revisions[self.applied - 1].forward)
    }

    fn fresh_id(&mut self) -> RevisionId {
        let id = RevisionId(self.next_id);
        self.next_id += 1;
        id
    }
}
