//! A document: the text, its undo history and whether it differs from what was saved.

use ropey::Rope;

use crate::format::Format;
use crate::history::{History, RevisionId, UndoGrouping};
use crate::line_ending::LineEnding;
use crate::selection::Selection;
use crate::transaction::Transaction;

/// The text of one open file (or an unsaved "new 1" tab).
///
/// A document has no cursor: carets and scroll position belong to the views showing it, and
/// several views can show the same document (Notepad++'s "Clone to Other View"). After every
/// change, views map their selections through the transaction's change set.
#[derive(Debug, Clone)]
pub struct Document {
    text: Rope,
    history: History,
    saved: RevisionId,
    format: Format,
}

impl Default for Document {
    fn default() -> Self {
        Self::from_text(Rope::new())
    }
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    /// Wraps loaded text as UTF-8. Its line ending is detected from the first line break,
    /// falling back to the platform default.
    pub fn from_text(text: Rope) -> Self {
        Self::with_format(text, Format::new())
    }

    /// Wraps text loaded in `format`. The line-ending mode is detected from the first line break;
    /// `format.line_ending` is used only when the text has none, or when it is NEL (an EBCDIC
    /// file's, already LF in the text).
    pub fn with_format(text: Rope, format: Format) -> Self {
        let line_ending = match format.line_ending {
            LineEnding::Nel => LineEnding::Nel,
            other => LineEnding::detect(&text).unwrap_or(other),
        };
        Self {
            text,
            history: History::new(),
            saved: RevisionId::INITIAL,
            format: format.with_line_ending(line_ending),
        }
    }

    pub fn text(&self) -> &Rope {
        &self.text
    }

    pub fn format(&self) -> Format {
        self.format
    }

    pub fn line_ending(&self) -> LineEnding {
        self.format.line_ending
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    /// Applies `transaction` and records it in the undo history.
    ///
    /// # Panics
    ///
    /// Panics if the transaction was built for a different version of the text.
    pub fn apply(
        &mut self,
        transaction: &Transaction,
        selection_before: &Selection,
        grouping: UndoGrouping,
    ) {
        let original = self.text.clone();
        let original_format = self.format;
        self.apply_to_state(transaction);
        self.history.commit(
            transaction.clone(),
            &original,
            original_format,
            selection_before,
            grouping,
        );
    }

    /// Undoes the last step and returns what was applied, so views can update.
    pub fn undo(&mut self) -> Option<Transaction> {
        let transaction = self.history.undo()?.clone();
        self.apply_to_state(&transaction);
        Some(transaction)
    }

    /// Redoes the next step and returns what was applied, so views can update.
    pub fn redo(&mut self) -> Option<Transaction> {
        let transaction = self.history.redo()?.clone();
        self.apply_to_state(&transaction);
        Some(transaction)
    }

    fn apply_to_state(&mut self, transaction: &Transaction) {
        transaction.changes().apply(&mut self.text);
        if let Some(format) = transaction.format() {
            self.format = format;
        }
    }

    /// True if the text differs from the last saved state. Undoing back to the saved state makes
    /// the document unmodified again.
    pub fn is_modified(&self) -> bool {
        self.history.current() != self.saved
    }

    /// A transaction converting every line break to `target` and making it the line-ending
    /// mode, as one undo step (Edit > EOL Conversion).
    pub fn convert_line_endings(&self, target: LineEnding) -> Transaction {
        let edits = crate::line_ending::convert_line_breaks(&self.text, target);
        Transaction::from_edits(&self.text, edits)
            .expect("line breaks lie on character boundaries")
            .with_format(self.format.with_line_ending(target))
    }

    /// Marks the current state as saved.
    pub fn mark_saved(&mut self) {
        self.saved = self.history.current();
    }

    /// The current state of the history. Saving in the background records the revision it
    /// wrote, so edits made while writing keep the document modified.
    pub fn revision(&self) -> RevisionId {
        self.history.current()
    }

    /// Marks `revision` as the state on disk.
    pub fn mark_saved_at(&mut self, revision: RevisionId) {
        self.saved = revision;
    }

    /// Records that no state of the history is on disk, as for text restored from a backup
    /// copy of unsaved changes: the document stays modified even after undoing everything.
    pub fn mark_unsaved(&mut self) {
        self.saved = RevisionId::NEVER;
    }
}

#[cfg(test)]
#[path = "../tests/unit/document/tests.rs"]
mod tests;
