//! Transactions: a change set plus the selection the change leaves behind.

use ropey::Rope;

use crate::change::{ChangeSet, Edit, InvalidEdit};
use crate::selection::{Range, Selection};

/// One editing action: what changes in the text and, optionally, where the selection ends up.
///
/// Every edit with several carets is a single transaction, so it is also a single undo step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub(crate) changes: ChangeSet,
    pub(crate) selection: Option<Selection>,
}

impl Transaction {
    pub fn new(changes: ChangeSet) -> Self {
        Self {
            changes,
            selection: None,
        }
    }

    pub fn from_edits<I>(text: &Rope, edits: I) -> Result<Self, InvalidEdit>
    where
        I: IntoIterator<Item = Edit>,
    {
        ChangeSet::from_edits(text, edits).map(Self::new)
    }

    /// Replaces every range of `selection` with `replacement` and leaves a caret after each
    /// inserted copy. This is typing, pasting and deleting the selection with any number of
    /// carets.
    pub fn replace_selections(
        text: &Rope,
        selection: &Selection,
        replacement: &str,
    ) -> Result<Self, InvalidEdit> {
        let edits = selection
            .iter()
            .map(|range| Edit::replace(range.from()..range.to(), replacement));
        let changes = ChangeSet::from_edits(text, edits)?;

        let mut added = 0;
        let mut removed = 0;
        let carets = selection.iter().map(|range| {
            let start = range.from() + added - removed;
            added += replacement.len();
            removed += range.len();
            Range::point(start + replacement.len())
        });
        let selection = Selection::new(carets, selection.primary_index());

        Ok(Self::new(changes).with_selection(selection))
    }

    pub fn with_selection(mut self, selection: Selection) -> Self {
        self.selection = Some(selection);
        self
    }

    pub fn changes(&self) -> &ChangeSet {
        &self.changes
    }

    pub fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_with_several_carets() {
        let mut text = Rope::from_str("ab\ncd\nef");
        let selection = Selection::new([Range::point(0), Range::point(3), Range::new(6, 8)], 2);
        let transaction = Transaction::replace_selections(&text, &selection, "> ").unwrap();
        transaction.changes().apply(&mut text);

        assert_eq!(text, "> ab\n> cd\n> ");
        let carets = transaction.selection().unwrap();
        assert_eq!(
            carets.ranges(),
            [Range::point(2), Range::point(7), Range::point(12)]
        );
        assert_eq!(carets.primary_index(), 2);
    }

    #[test]
    fn deleting_adjacent_ranges_merges_carets() {
        let mut text = Rope::from_str("abcd");
        let selection = Selection::new([Range::new(0, 2), Range::new(2, 4)], 0);
        let transaction = Transaction::replace_selections(&text, &selection, "").unwrap();
        transaction.changes().apply(&mut text);

        assert_eq!(text, "");
        assert_eq!(transaction.selection().unwrap().ranges(), [Range::point(0)]);
    }
}
