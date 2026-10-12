//! Increase and Decrease Line Indent (Tab and Shift+Tab on lines): the lines of every
//! selection move one indentation level right or left.

use ropey::Rope;

use super::line_blocks;
use crate::change::{Assoc, Edit};
use crate::motion::line_range;
use crate::selection::{Range as Span, Selection};
use crate::transaction::Transaction;

/// Puts `unit` (a tab, or spaces) at the start of every line of every selection; empty lines
/// stay empty. Selections that start at a line's start keep starting there; carets follow
/// their text.
pub fn indent_lines(text: &Rope, selection: &Selection, unit: &str) -> Option<Transaction> {
    if unit.is_empty() {
        return None;
    }
    let edits: Vec<Edit> = line_blocks(text, selection)
        .into_iter()
        .flatten()
        .map(|line| line_range(text, line))
        .filter(|content| !content.is_empty())
        .map(|content| Edit::insert(content.start, unit))
        .collect();
    finish(text, selection, edits)
}

/// Takes one indentation level off the start of every line of every selection: a tab, or the
/// spaces back to the previous multiple of `tab_width`. Lines without indentation stay.
pub fn outdent_lines(text: &Rope, selection: &Selection, tab_width: usize) -> Option<Transaction> {
    let tab_width = tab_width.max(1);
    let edits: Vec<Edit> = line_blocks(text, selection)
        .into_iter()
        .flatten()
        .filter_map(|line| {
            let start = line_range(text, line).start;
            let removed = outdent_width(text.slice(line_range(text, line)).chars(), tab_width);
            (removed > 0).then(|| Edit::delete(start..start + removed))
        })
        .collect();
    finish(text, selection, edits)
}

/// How many bytes Decrease Line Indent takes off a line starting with `chars`.
fn outdent_width(mut chars: impl Iterator<Item = char>, tab_width: usize) -> usize {
    match chars.next() {
        Some('\t') => 1,
        Some(' ') => {
            let spaces = 1 + chars.take_while(|c| *c == ' ').count();
            match spaces % tab_width {
                0 => tab_width,
                rest => rest,
            }
        }
        _ => 0,
    }
}

/// One transaction for `edits`; a caret stays with the text it is in, a selection keeps its
/// start at a line's start.
fn finish(text: &Rope, selection: &Selection, edits: Vec<Edit>) -> Option<Transaction> {
    if edits.is_empty() {
        return None;
    }
    let transaction = Transaction::from_edits(text, edits).ok()?;
    let changes = transaction.changes();
    let ranges: Vec<Span> = selection
        .iter()
        .map(|range| {
            if range.is_empty() {
                Span::point(changes.map_pos(range.head, Assoc::After))
            } else {
                let map = |pos: usize, assoc| changes.map_pos(pos, assoc);
                let (from, to) = (
                    map(range.from(), Assoc::Before),
                    map(range.to(), Assoc::After),
                );
                if range.anchor <= range.head {
                    Span::new(from, to)
                } else {
                    Span::new(to, from)
                }
            }
        })
        .collect();
    let mapped = Selection::new(ranges, selection.primary_index());
    Some(transaction.with_selection(mapped))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::run;

    #[test]
    fn indent_the_selected_lines() {
        assert_eq!(
            run("[a\nb\n]c", |t, s| indent_lines(t, s, "\t")),
            "[\ta\n\tb\n]c",
            "the line the selection ends at the start of stays"
        );
        assert_eq!(run("a|b", |t, s| indent_lines(t, s, "    ")), "    a|b");
        assert_eq!(run("|ab", |t, s| indent_lines(t, s, "  ")), "  |ab");
        // Empty lines stay empty; two carets on one line indent it once.
        assert_eq!(
            run("[a\n\nb]", |t, s| indent_lines(t, s, "\t")),
            "[\ta\n\n\tb]"
        );
        assert_eq!(run("a|b|c", |t, s| indent_lines(t, s, "\t")), "\ta|b|c");
        // CRLF, wide characters, the last line without a break.
        assert_eq!(
            run("[é\r\n日本]", |t, s| indent_lines(t, s, "\t")),
            "[\té\r\n\t日本]"
        );
    }

    #[test]
    fn indent_with_nothing_to_do() {
        assert_eq!(run("|", |t, s| indent_lines(t, s, "\t")), "|");
        assert_eq!(run("a|", |t, s| indent_lines(t, s, "")), "a|");
    }

    #[test]
    fn outdent_by_a_tab_or_to_the_previous_stop() {
        assert_eq!(run("\ta|", |t, s| outdent_lines(t, s, 4)), "a|");
        assert_eq!(run("        a|", |t, s| outdent_lines(t, s, 4)), "    a|");
        assert_eq!(run("      a|", |t, s| outdent_lines(t, s, 4)), "    a|");
        assert_eq!(run("  a|", |t, s| outdent_lines(t, s, 4)), "a|");
        assert_eq!(run(" \ta|", |t, s| outdent_lines(t, s, 4)), "\ta|");
        assert_eq!(
            run("[\ta\nb\n    c]", |t, s| outdent_lines(t, s, 4)),
            "[a\nb\nc]"
        );
        // A caret inside the indentation goes to the line's start.
        assert_eq!(run("  |  a", |t, s| outdent_lines(t, s, 4)), "|a");
    }

    #[test]
    fn outdent_with_nothing_to_do() {
        assert_eq!(run("a|\nb", |t, s| outdent_lines(t, s, 4)), "a|\nb");
        assert_eq!(run("|", |t, s| outdent_lines(t, s, 4)), "|");
        // A tab width of zero counts as one.
        assert_eq!(run("  a|", |t, s| outdent_lines(t, s, 0)), " a|");
        assert_eq!(outdent_width("x".chars(), 4), 0);
        assert_eq!(outdent_width("".chars(), 4), 0);
    }
}
