//! Auto-closing brackets and quotes, as Notepad++'s Preferences > Auto-Completion > Auto-Insert:
//! typing an opening bracket inserts its partner after the caret, typing the closing one over a
//! partner steps over it, and Backspace between an empty pair deletes both.

use ropey::Rope;

use crate::change::Edit;
use crate::selection::{Range as Span, Selection};
use crate::transaction::Transaction;

/// The pairs to complete, each `(open, close)`; quotes have the same character twice.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pairs(pub Vec<(char, char)>);

impl Pairs {
    fn closing_of(&self, open: char) -> Option<char> {
        self.0
            .iter()
            .find(|(opening, _)| *opening == open)
            .map(|(_, close)| *close)
    }

    fn is_closing(&self, ch: char) -> bool {
        self.0.iter().any(|(_, close)| *close == ch)
    }
}

fn char_after(text: &Rope, pos: usize) -> Option<char> {
    (pos < text.len())
        .then(|| text.slice(pos..).chars().next())
        .flatten()
}

fn char_before(text: &Rope, pos: usize) -> Option<char> {
    (pos > 0)
        .then(|| {
            text.slice(text.floor_char_boundary(pos - 1)..pos)
                .chars()
                .next()
        })
        .flatten()
}

fn is_word(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// What typing `typed` does at a caret.
enum Action {
    /// Insert the pair, the caret between.
    Pair(char),
    /// Step over the closing character after the caret.
    StepOver,
    Type,
}

fn action(text: &Rope, pos: usize, typed: char, pairs: &Pairs) -> Action {
    let after = char_after(text, pos);
    if after == Some(typed) && pairs.is_closing(typed) {
        return Action::StepOver;
    }
    let Some(close) = pairs.closing_of(typed) else {
        return Action::Type;
    };
    // Only before nothing, white space or a closing character, so that a bracket typed before a
    // word does not get a partner in the way.
    let free_after = after.is_none_or(|ch| ch.is_whitespace() || pairs.is_closing(ch));
    // A quote right after a word is an apostrophe or ends a string: no pair.
    let quote = typed == close;
    let free_before = !quote || char_before(text, pos).is_none_or(|ch| !is_word(ch) && ch != typed);
    if free_after && free_before {
        Action::Pair(close)
    } else {
        Action::Type
    }
}

/// Typing `typed` with every caret empty: `None` when no caret pairs or steps over, so that the
/// character is typed as usual.
pub fn type_with_pairs(
    text: &Rope,
    selection: &Selection,
    typed: char,
    pairs: &Pairs,
) -> Option<Transaction> {
    if selection.iter().any(|range| !range.is_empty()) {
        return None;
    }
    let actions: Vec<Action> = selection
        .iter()
        .map(|range| action(text, range.head, typed, pairs))
        .collect();
    if actions.iter().all(|action| matches!(action, Action::Type)) {
        return None;
    }
    let mut edits = Vec::new();
    let mut ranges = Vec::new();
    let mut shift = 0usize;
    for (range, action) in selection.iter().zip(actions) {
        let pos = range.head;
        let caret = match action {
            Action::Pair(close) => {
                edits.push(Edit::insert(pos, format!("{typed}{close}")));
                shift += typed.len_utf8() + close.len_utf8();
                pos + shift - close.len_utf8()
            }
            Action::StepOver => pos + shift + typed.len_utf8(),
            Action::Type => {
                edits.push(Edit::insert(pos, typed.to_string()));
                shift += typed.len_utf8();
                pos + shift
            }
        };
        ranges.push(Span::point(caret));
    }
    let transaction = Transaction::from_edits(text, edits).ok()?;
    Some(transaction.with_selection(Selection::new(ranges, selection.primary_index())))
}

/// Backspace with every caret empty and between an empty pair: deletes both characters of
/// each pair. `None` when a caret is not between a pair, for the usual Backspace.
pub fn delete_pairs(text: &Rope, selection: &Selection, pairs: &Pairs) -> Option<Transaction> {
    let mut edits = Vec::new();
    let mut ranges = Vec::new();
    let mut removed = 0;
    for range in selection.iter() {
        if !range.is_empty() {
            return None;
        }
        let pos = range.head;
        let (open, close) = (char_before(text, pos)?, char_after(text, pos)?);
        if pairs.closing_of(open) != Some(close) {
            return None;
        }
        let start = pos - open.len_utf8();
        edits.push(Edit::delete(start..pos + close.len_utf8()));
        ranges.push(Span::point(start - removed));
        removed += open.len_utf8() + close.len_utf8();
    }
    let transaction = Transaction::from_edits(text, edits).ok()?;
    Some(transaction.with_selection(Selection::new(ranges, selection.primary_index())))
}
