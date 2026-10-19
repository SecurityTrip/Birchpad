//! Closing HTML and XML tags as their opening tag is typed (Auto-Completion > Close HTML and XML
//! tags).

use ropey::Rope;

use crate::change::Edit;
use crate::completion::closing_tag;
use crate::selection::{Range as Span, Selection};
use crate::transaction::Transaction;

/// How far back from a caret an opening tag is looked for: a tag longer than this is not
/// closed.
const TAG_REACH: usize = 4096;

/// After `>` was typed at every caret: inserts the closing tag of the opening tag each caret
/// ends, after the caret, which stays where it is. `None` when no caret ends an opening tag
/// (`html` leaves out HTML's void elements).
pub fn close_tags(text: &Rope, selection: &Selection, html: bool) -> Option<Transaction> {
    if selection.iter().any(|range| !range.is_empty()) {
        return None;
    }
    let mut edits = Vec::new();
    let mut carets = Vec::new();
    let mut shift = 0;
    for range in selection.iter() {
        let pos = range.head;
        let mut from = pos.saturating_sub(TAG_REACH);
        while !text.is_char_boundary(from) {
            from += 1;
        }
        let before = text.slice(from..pos).to_string();
        carets.push(Span::point(pos + shift));
        if let Some(tag) = closing_tag(&before, html) {
            shift += tag.len();
            edits.push(Edit::insert(pos, tag));
        }
    }
    if edits.is_empty() {
        return None;
    }
    let transaction = Transaction::from_edits(text, edits).ok()?;
    // The carets stay before the inserted tags.
    Some(transaction.with_selection(Selection::new(carets, selection.primary_index())))
}

#[cfg(test)]
#[path = "../../tests/unit/ops/tags/tests.rs"]
mod tests;
