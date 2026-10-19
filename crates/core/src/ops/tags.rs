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
mod tests {
    use super::*;

    fn close(text: &str, carets: &[usize], html: bool) -> Option<(String, Vec<usize>)> {
        let mut rope = Rope::from_str(text);
        let selection = Selection::new(carets.iter().map(|&at| Span::point(at)), 0);
        let transaction = close_tags(&rope, &selection, html)?;
        transaction.changes().apply(&mut rope);
        let heads = transaction
            .selection()
            .expect("set")
            .iter()
            .map(|range| range.head)
            .collect();
        Some((rope.to_string(), heads))
    }

    #[test]
    fn the_tag_typed_is_closed_after_the_caret() {
        assert_eq!(
            close("<p>", &[3], true),
            Some(("<p></p>".to_owned(), vec![3]))
        );
        // Two carets, each in its own tag: the second moves with the first tag inserted.
        assert_eq!(
            close("<a>\n<b>", &[3, 7], false),
            Some(("<a></a>\n<b></b>".to_owned(), vec![3, 11]))
        );
    }

    #[test]
    fn nothing_to_close() {
        assert_eq!(close("<br>", &[4], true), None, "a void element");
        assert_eq!(close("</p>", &[4], true), None);
        assert_eq!(close("a > b", &[3], true), None);
        assert_eq!(close("", &[0], true), None);
        let rope = Rope::from_str("<p>");
        let selected = Selection::new(vec![Span::new(0, 3)], 0);
        assert!(close_tags(&rope, &selected, true).is_none(), "a selection");
    }

    #[test]
    fn a_tag_beyond_reach_is_left_alone() {
        let long = format!("<p title=\"{}\">", "x".repeat(TAG_REACH));
        assert_eq!(close(&long, &[long.len()], true), None);
        let just = format!("<p{}>", " ".repeat(TAG_REACH - 3));
        assert!(close(&just, &[just.len()], true).is_some());
    }
}
