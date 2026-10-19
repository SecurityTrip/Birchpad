//! Transactions: a change set plus the selection the change leaves behind.

use ropey::Rope;

use crate::change::{ChangeSet, Edit, InvalidEdit};
use crate::format::Format;
use crate::selection::{Range, Selection};

/// One editing action: what changes in the text, optionally where the selection ends up, and
/// optionally a new format (encoding, BOM, line-ending mode) for the document.
///
/// Every edit with several carets is a single transaction, so it is also a single undo step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub(crate) changes: ChangeSet,
    pub(crate) selection: Option<Selection>,
    pub(crate) format: Option<Format>,
}

impl Transaction {
    pub fn new(changes: ChangeSet) -> Self {
        Self {
            changes,
            selection: None,
            format: None,
        }
    }

    /// A transaction that only changes the document's format, e.g. "Convert to UTF-8".
    pub fn set_format(len: usize, format: Format) -> Self {
        Self::new(ChangeSet::identity(len)).with_format(format)
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
        Self::replace_selections_with(text, selection, |_| replacement.to_owned())
    }

    /// Replaces every range of `selection` with its own text (e.g. a different number of spaces
    /// per caret to reach the next tab stop) and leaves a caret after each.
    ///
    /// A range that starts in virtual space gets that space filled with spaces before its text,
    /// as typing past the end of a line in a rectangular selection does. Deleting (an empty
    /// replacement) leaves the caret where the range started, virtual space included.
    pub fn replace_selections_with(
        text: &Rope,
        selection: &Selection,
        mut replacement: impl FnMut(Range) -> String,
    ) -> Result<Self, InvalidEdit> {
        let replacements: Vec<(Range, String)> = selection
            .iter()
            .map(|&range| {
                let mut inserted = replacement(range);
                if !inserted.is_empty() && range.from_virtual() > 0 {
                    inserted.insert_str(0, &" ".repeat(range.from_virtual()));
                }
                (range, inserted)
            })
            .collect();
        let edits = replacements
            .iter()
            .map(|(range, text)| Edit::replace(range.from()..range.to(), text.clone()));
        let changes = ChangeSet::from_edits(text, edits)?;

        let mut added = 0;
        let mut removed = 0;
        let carets = replacements.iter().map(|(range, inserted)| {
            let start = range.from() + added - removed;
            added += inserted.len();
            removed += range.len();
            if inserted.is_empty() {
                Range::virtual_point(start, range.from_virtual())
            } else {
                Range::point(start + inserted.len())
            }
        });
        let selection = Selection::new(carets, selection.primary_index());

        Ok(Self::new(changes).with_selection(selection))
    }

    pub fn with_selection(mut self, selection: Selection) -> Self {
        self.selection = Some(selection);
        self
    }

    pub fn with_format(mut self, format: Format) -> Self {
        self.format = Some(format);
        self
    }

    pub fn changes(&self) -> &ChangeSet {
        &self.changes
    }

    pub fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }

    /// The format the document has after this transaction, if it changes.
    pub fn format(&self) -> Option<Format> {
        self.format
    }
}

#[cfg(test)]
#[path = "../tests/unit/transaction/tests.rs"]
mod tests;
