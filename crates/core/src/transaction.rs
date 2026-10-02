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
    fn each_caret_can_insert_different_text() {
        let mut text = Rope::from_str("a\nbcd");
        let selection = Selection::new([Range::point(1), Range::point(5)], 0);
        let transaction = Transaction::replace_selections_with(&text, &selection, |range| {
            " ".repeat(range.head % 3)
        })
        .unwrap();
        transaction.changes().apply(&mut text);
        assert_eq!(text, "a \nbcd  ");
        assert_eq!(
            transaction.selection().unwrap().ranges(),
            [Range::point(2), Range::point(8)]
        );
    }

    #[test]
    fn typing_in_virtual_space_fills_it_with_spaces() {
        let mut text = Rope::from_str("abcd\nx\n");
        // A rectangle from column 2 to 3: "c" on the first line, virtual space on the second
        // (whose end is at byte 6), and an empty line in virtual space.
        let selection = Selection::new(
            [
                Range::new(2, 3),
                Range::new(6, 6).with_virtual(1, 2),
                Range::new(7, 7).with_virtual(2, 3),
            ],
            0,
        );
        let typed = Transaction::replace_selections(&text, &selection, "Z").unwrap();
        let mut typed_text = text.clone();
        typed.changes().apply(&mut typed_text);
        assert_eq!(typed_text, "abZd\nx Z\n  Z");
        assert_eq!(
            typed.selection().unwrap().ranges(),
            [Range::point(3), Range::point(8), Range::point(12)]
        );

        // Deleting keeps the carets at the left edge, in virtual space where it was.
        let deleted = Transaction::replace_selections(&text, &selection, "").unwrap();
        deleted.changes().apply(&mut text);
        assert_eq!(text, "abd\nx\n");
        assert_eq!(
            deleted.selection().unwrap().ranges(),
            [
                Range::point(2),
                Range::virtual_point(5, 1),
                Range::virtual_point(6, 2)
            ]
        );
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
