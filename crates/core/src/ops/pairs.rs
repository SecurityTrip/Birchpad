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

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Pairs {
        Pairs(vec![
            ('(', ')'),
            ('[', ']'),
            ('{', '}'),
            ('"', '"'),
            ('\'', '\''),
        ])
    }

    /// `text` with `|` for carets, typed `ch`: the result with its carets.
    fn typed(text: &str, ch: char) -> Option<String> {
        let (rope, selection) = parse(text);
        let transaction = type_with_pairs(&rope, &selection, ch, &all())?;
        Some(show(&rope, &transaction))
    }

    fn backspace(text: &str) -> Option<String> {
        let (rope, selection) = parse(text);
        let transaction = delete_pairs(&rope, &selection, &all())?;
        Some(show(&rope, &transaction))
    }

    fn parse(text: &str) -> (Rope, Selection) {
        let mut carets = Vec::new();
        let mut plain = String::new();
        for ch in text.chars() {
            if ch == '|' {
                carets.push(Span::point(plain.len()));
            } else {
                plain.push(ch);
            }
        }
        (Rope::from_str(&plain), Selection::new(carets, 0))
    }

    fn show(rope: &Rope, transaction: &Transaction) -> String {
        let mut result = rope.clone();
        transaction.changes().apply(&mut result);
        let mut text = result.to_string();
        let mut heads: Vec<usize> = transaction
            .selection()
            .unwrap()
            .iter()
            .map(|range| range.head)
            .collect();
        heads.sort_unstable();
        for head in heads.into_iter().rev() {
            text.insert(head, '|');
        }
        text
    }

    #[test]
    fn opening_brackets_get_their_partner() {
        assert_eq!(typed("f|", '(').as_deref(), Some("f(|)"));
        assert_eq!(typed("|", '[').as_deref(), Some("[|]"));
        assert_eq!(typed("a | b", '{').as_deref(), Some("a {|} b"));
        assert_eq!(typed("(|)", '(').as_deref(), Some("((|))"));
        // Before a word, the bracket is typed alone.
        assert_eq!(typed("|word", '('), None);
    }

    #[test]
    fn closing_characters_step_over() {
        assert_eq!(typed("f(|)", ')').as_deref(), Some("f()|"));
        assert_eq!(typed("\"a|\"", '"').as_deref(), Some("\"a\"|"));
        assert_eq!(typed("|x", ')'), None);
        assert_eq!(typed("|", ')'), None);
    }

    #[test]
    fn quotes_pair_only_where_a_string_starts() {
        assert_eq!(typed("x = |", '"').as_deref(), Some("x = \"|\""));
        // An apostrophe after a word.
        assert_eq!(typed("don|", '\''), None);
        assert_eq!(typed("é|", '\''), None);
        assert_eq!(typed("x = |", '\'').as_deref(), Some("x = '|'"));
    }

    #[test]
    fn every_caret_at_once() {
        assert_eq!(typed("a|\nb|\nc", '(').as_deref(), Some("a(|)\nb(|)\nc"));
        // Carets that pair and carets that type.
        assert_eq!(typed("|x\n|", '(').as_deref(), Some("(|x\n(|)"));
        // Wide characters and the end of the text.
        assert_eq!(typed("日|", '[').as_deref(), Some("日[|]"));
    }

    #[test]
    fn a_selection_types_as_usual() {
        let rope = Rope::from_str("abc");
        let selection = Selection::new(vec![Span::new(0, 2)], 0);
        assert!(type_with_pairs(&rope, &selection, '(', &all()).is_none());
        assert!(delete_pairs(&rope, &selection, &all()).is_none());
        // No pairs set: nothing pairs.
        let rope = Rope::from_str("");
        assert!(type_with_pairs(&rope, &Selection::point(0), '(', &Pairs::default()).is_none());
    }

    #[test]
    fn backspace_deletes_an_empty_pair() {
        assert_eq!(backspace("f(|)").as_deref(), Some("f|"));
        assert_eq!(backspace("\"|\"").as_deref(), Some("|"));
        assert_eq!(backspace("[|]\n{|}").as_deref(), Some("|\n|"));
        assert_eq!(backspace("(|x)"), None);
        assert_eq!(backspace("|"), None);
        assert_eq!(backspace("(|"), None);
        assert_eq!(backspace("(|]"), None);
        // One caret not in a pair: the usual Backspace for all.
        assert_eq!(backspace("(|)\nx|"), None);
    }
}
