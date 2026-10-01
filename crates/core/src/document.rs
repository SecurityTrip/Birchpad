//! A document: the text, its undo history and whether it differs from what was saved.

use ropey::Rope;

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
    line_ending: LineEnding,
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

    /// Wraps loaded text. Its line ending is detected from the first line break, falling back to
    /// the platform default.
    pub fn from_text(text: Rope) -> Self {
        let line_ending = LineEnding::detect(&text).unwrap_or_else(LineEnding::native);
        Self {
            text,
            history: History::new(),
            saved: RevisionId::INITIAL,
            line_ending,
        }
    }

    pub fn text(&self) -> &Rope {
        &self.text
    }

    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
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
        transaction.changes().apply(&mut self.text);
        self.history
            .commit(transaction.clone(), &original, selection_before, grouping);
    }

    /// Undoes the last step and returns what was applied, so views can update.
    pub fn undo(&mut self) -> Option<Transaction> {
        let transaction = self.history.undo()?.clone();
        transaction.changes().apply(&mut self.text);
        Some(transaction)
    }

    /// Redoes the next step and returns what was applied, so views can update.
    pub fn redo(&mut self) -> Option<Transaction> {
        let transaction = self.history.redo()?.clone();
        transaction.changes().apply(&mut self.text);
        Some(transaction)
    }

    /// True if the text differs from the last saved state. Undoing back to the saved state makes
    /// the document unmodified again.
    pub fn is_modified(&self) -> bool {
        self.history.current() != self.saved
    }

    /// Marks the current state as saved.
    pub fn mark_saved(&mut self) {
        self.saved = self.history.current();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change::Edit;
    use crate::selection::Range;

    fn type_text(
        doc: &mut Document,
        selection: &Selection,
        text: &str,
        grouping: UndoGrouping,
    ) -> Selection {
        let transaction = Transaction::replace_selections(doc.text(), selection, text).unwrap();
        doc.apply(&transaction, selection, grouping);
        transaction.selection().unwrap().clone()
    }

    #[test]
    fn merged_typing_is_one_undo_step() {
        let mut doc = Document::new();
        let mut selection = Selection::point(0);
        for ch in ["h", "e", "y"] {
            selection = type_text(&mut doc, &selection, ch, UndoGrouping::MergeWithPrevious);
        }
        assert_eq!(doc.text(), "hey");
        assert_eq!(selection.primary(), Range::point(3));

        let undone = doc.undo().unwrap();
        assert_eq!(doc.text(), "");
        assert_eq!(undone.selection().unwrap().primary(), Range::point(0));
        assert!(doc.undo().is_none());
    }

    #[test]
    fn undo_back_to_saved_state_is_unmodified() {
        let mut doc = Document::from_text(Rope::from_str("saved"));
        assert!(!doc.is_modified());

        let selection = type_text(&mut doc, &Selection::point(5), "!", UndoGrouping::NewStep);
        assert!(doc.is_modified());
        doc.mark_saved();
        assert!(!doc.is_modified());

        doc.undo();
        assert!(doc.is_modified());
        doc.redo();
        assert!(!doc.is_modified());

        // Typing more merges into the saved step, which must still count as a modification.
        type_text(&mut doc, &selection, "?", UndoGrouping::MergeWithPrevious);
        assert_eq!(doc.text(), "saved!?");
        assert!(doc.is_modified());
    }

    #[test]
    fn new_edit_after_undo_discards_redo() {
        let mut doc = Document::from_text(Rope::from_str("abc"));
        let transaction = Transaction::from_edits(doc.text(), [Edit::delete(0..1)]).unwrap();
        doc.apply(&transaction, &Selection::point(0), UndoGrouping::NewStep);
        doc.undo();
        assert!(doc.history().can_redo());

        let transaction = Transaction::from_edits(doc.text(), [Edit::insert(3, "d")]).unwrap();
        doc.apply(
            &transaction,
            &Selection::point(3),
            UndoGrouping::MergeWithPrevious,
        );
        assert!(!doc.history().can_redo());
        assert_eq!(doc.text(), "abcd");
    }
}
