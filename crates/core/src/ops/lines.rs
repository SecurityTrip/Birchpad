//! Operations on the current lines: duplicate, delete, move, insert a blank line, join, split.

use std::collections::HashMap;

use ropey::Rope;

use super::{line_blocks, line_texts, replace_block, target_lines};
use crate::change::Edit;
use crate::line_ending::LineEnding;
use crate::motion::{line_count, line_of, line_range, line_range_with_break};
use crate::selection::{Range as Span, Selection};
use crate::transaction::Transaction;

/// Ctrl+D: duplicates each selection, or the line of each caret, after itself. Carets and
/// selections stay on the original, as in Scintilla.
pub fn duplicate(text: &Rope, selection: &Selection, eol: LineEnding) -> Option<Transaction> {
    let mut edits = Vec::new();
    let mut last_line = None;
    for range in selection.iter() {
        if range.is_empty() {
            let line = line_of(text, range.head);
            if last_line == Some(line) {
                continue;
            }
            last_line = Some(line);
            let content = line_range(text, line);
            let copy = format!("{}{}", eol.as_str(), text.slice(content.clone()));
            edits.push(Edit::insert(content.end, copy));
        } else {
            let copy = text.slice(range.from()..range.to()).to_string();
            edits.push(Edit::insert(range.to(), copy));
        }
    }
    edits.sort_by_key(|edit| edit.range.start);
    Transaction::from_edits(text, edits).ok()
}

/// Ctrl+Shift+L: deletes the lines of every selection. Deleting the last line also takes the
/// line break before it, so no empty line is left behind.
pub fn delete_lines(text: &Rope, selection: &Selection) -> Option<Transaction> {
    let edits: Vec<Edit> = line_blocks(text, selection)
        .into_iter()
        .map(|lines| {
            let start = line_range_with_break(text, lines.start).start;
            let end = line_range_with_break(text, lines.end - 1).end;
            let at_end = lines.end == line_count(text) && lines.start > 0;
            let start = if at_end {
                line_range(text, lines.start - 1).end
            } else {
                start
            };
            Edit::delete(start..end)
        })
        .filter(|edit| !edit.range.is_empty())
        .collect();
    if edits.is_empty() {
        return None;
    }
    Transaction::from_edits(text, edits).ok()
}

/// Ctrl+Shift+Up / Down: moves the lines of every selection one line up or down, selections
/// with them. Does nothing if a block is already at the edge.
pub fn move_lines(
    text: &Rope,
    selection: &Selection,
    up: bool,
    eol: LineEnding,
) -> Option<Transaction> {
    let blocks = line_blocks(text, selection);
    let lines = line_count(text);
    if blocks.iter().any(|block| {
        if up {
            block.start == 0
        } else {
            block.end >= lines
        }
    }) {
        return None;
    }
    let eol = eol.as_str();
    let mut edits = Vec::new();
    // Where each moved line starts in the new text.
    let mut new_starts: HashMap<usize, usize> = HashMap::new();
    // Net growth of the edits so far (line breaks inside a block may change length).
    let mut growth: isize = 0;
    for block in &blocks {
        let swapped = if up { block.start - 1 } else { block.end };
        let span = if up {
            block.start - 1..block.end
        } else {
            block.start..block.end + 1
        };
        let range = super::block_range(text, span);
        let block_lines = line_texts(text, block.clone());
        let other = text.slice(line_range(text, swapped)).to_string();
        let mut new_lines = Vec::with_capacity(block_lines.len() + 1);
        if !up {
            new_lines.push(other.clone());
        }
        new_lines.extend(block_lines.iter().cloned());
        if up {
            new_lines.push(other);
        }
        let new_start = (range.start as isize + growth) as usize;
        let mut offset = new_start
            + if up {
                0
            } else {
                new_lines[0].len() + eol.len()
            };
        for (line, content) in block.clone().zip(&block_lines) {
            new_starts.insert(line, offset);
            offset += content.len() + eol.len();
        }
        let replacement = new_lines.join(eol);
        growth += replacement.len() as isize - range.len() as isize;
        edits.push(Edit::replace(range, replacement));
    }
    let transaction = Transaction::from_edits(text, edits).ok()?;
    let relocate = |pos: usize| {
        let line = line_of(text, pos);
        let column = pos - line_range(text, line).start;
        match new_starts.get(&line) {
            Some(start) => start + column,
            // A selection ending at the start of the line after a block: the end of the
            // moved block's last line, past its line break.
            None => {
                let last = line - 1;
                let end_of_last = new_starts[&last] + line_range(text, last).len();
                end_of_last + eol.len()
            }
        }
    };
    let ranges: Vec<Span> = selection
        .iter()
        .map(|range| Span::new(relocate(range.anchor), relocate(range.head)))
        .collect();
    Some(transaction.with_selection(Selection::new(ranges, selection.primary_index())))
}

/// Ctrl+Alt+Enter / Ctrl+Alt+Shift+Enter: inserts an empty line above or below the line of
/// each caret and moves the caret onto it.
pub fn insert_blank_line(
    text: &Rope,
    selection: &Selection,
    above: bool,
    eol: LineEnding,
) -> Option<Transaction> {
    let mut lines: Vec<usize> = selection
        .iter()
        .map(|range| line_of(text, range.head))
        .collect();
    lines.dedup();
    let eol = eol.as_str();
    let mut edits = Vec::new();
    let mut carets = Vec::new();
    let mut added = 0;
    for line in lines {
        let content = line_range(text, line);
        let at = if above { content.start } else { content.end };
        edits.push(Edit::insert(at, eol));
        let caret = if above { at } else { at + eol.len() };
        carets.push(Span::point(caret + added));
        added += eol.len();
    }
    let transaction = Transaction::from_edits(text, edits).ok()?;
    Some(transaction.with_selection(Selection::new(carets, selection.primary_index())))
}

/// Ctrl+J: joins the selected lines into one (the caret's line and the next without a
/// selection). Leading blanks of joined lines go; a space separates the parts.
pub fn join_lines(text: &Rope, selection: &Selection, eol: LineEnding) -> Option<Transaction> {
    let mut lines = target_lines(text, selection);
    if selection.iter().all(|range| range.is_empty()) {
        let line = line_of(text, selection.primary().head);
        lines = line..(line + 2).min(line_count(text));
    }
    if lines.len() < 2 {
        return None;
    }
    let mut joined = String::new();
    for (index, content) in line_texts(text, lines.clone()).into_iter().enumerate() {
        let part = if index == 0 {
            content.as_str()
        } else {
            content.trim_start_matches([' ', '\t'])
        };
        if index > 0 && !part.is_empty() && !joined.is_empty() && !joined.ends_with([' ', '\t']) {
            joined.push(' ');
        }
        joined.push_str(part);
    }
    replace_block(text, selection, lines, &[joined], eol)
}

/// Ctrl+I: splits the selected lines (the caret's line without a selection) so that none is
/// longer than `width` characters, breaking at spaces; a longer word stays whole.
pub fn split_lines(
    text: &Rope,
    selection: &Selection,
    width: usize,
    eol: LineEnding,
) -> Option<Transaction> {
    let lines = if selection.iter().all(|range| range.is_empty()) {
        let line = line_of(text, selection.primary().head);
        line..line + 1
    } else {
        target_lines(text, selection)
    };
    let width = width.max(1);
    let mut new_lines = Vec::new();
    for content in line_texts(text, lines.clone()) {
        let mut current = String::new();
        for word in content.split(' ') {
            let fits = current.chars().count() + 1 + word.chars().count() <= width;
            if !current.is_empty() && !fits {
                new_lines.push(std::mem::take(&mut current));
            } else if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
        new_lines.push(current);
    }
    replace_block(text, selection, lines, &new_lines, eol)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::run;

    const LF: LineEnding = LineEnding::Lf;

    #[test]
    fn duplicate_lines_and_selections() {
        assert_eq!(run("a|b\ncd", |t, s| duplicate(t, s, LF)), "a|b\nab\ncd");
        assert_eq!(run("ab\nc|d", |t, s| duplicate(t, s, LF)), "ab\nc|d\ncd");
        assert_eq!(run("[ab]c", |t, s| duplicate(t, s, LF)), "[ab]abc");
        // Two carets on one line duplicate it once.
        assert_eq!(run("|a|b", |t, s| duplicate(t, s, LF)), "|a|b\nab");
    }

    #[test]
    fn delete_lines_including_the_last() {
        assert_eq!(run("a\nb|b\nc", delete_lines), "a\n|c");
        assert_eq!(run("a\nb\nc|c", delete_lines), "a\nb|");
        assert_eq!(run("[a\nb\n]c", delete_lines), "|c");
        assert_eq!(run("on|ly", delete_lines), "|");
    }

    #[test]
    fn move_lines_carries_selections() {
        assert_eq!(
            run("a\nb|b\nc", |t, s| move_lines(t, s, true, LF)),
            "b|b\na\nc"
        );
        assert_eq!(
            run("a\nb|b\nc", |t, s| move_lines(t, s, false, LF)),
            "a\nc\nb|b"
        );
        assert_eq!(
            run("a\n[b\nc]\nd", |t, s| move_lines(t, s, true, LF)),
            "[b\nc]\na\nd"
        );
        // Whole lines selected down to the start of the next one, as with the mouse.
        assert_eq!(
            run("a\n[b\n]c", |t, s| move_lines(t, s, true, LF)),
            "[b\n]a\nc"
        );
        assert_eq!(
            run("[a\n]b\nc", |t, s| move_lines(t, s, false, LF)),
            "b\n[a\n]c"
        );
        // Mixed line breaks inside a moved block become the document's.
        assert_eq!(
            run("x\n|a\r\nb", |t, s| move_lines(t, s, true, LF)),
            "|a\nx\r\nb"
        );
        assert_eq!(
            run("|a\nb", |t, s| move_lines(t, s, true, LF)),
            "|a\nb",
            "at the top"
        );
        // Two separate blocks move together.
        assert_eq!(
            run("a\n|b\nc\n|d\ne", |t, s| move_lines(t, s, false, LF)),
            "a\nc\n|b\ne\n|d"
        );
    }

    #[test]
    fn blank_lines_above_and_below() {
        assert_eq!(
            run("a\nb|c", |t, s| insert_blank_line(t, s, true, LF)),
            "a\n|\nbc"
        );
        assert_eq!(
            run("a|b\nc", |t, s| insert_blank_line(t, s, false, LF)),
            "ab\n|\nc"
        );
    }

    #[test]
    fn join_and_split() {
        assert_eq!(
            run("a|b\n   cd\nef", |t, s| join_lines(t, s, LF)),
            "|ab cd\nef"
        );
        assert_eq!(
            run("[one\n  two\n\nthree]", |t, s| join_lines(t, s, LF)),
            "[one two three]"
        );
        assert_eq!(
            run("the quick| brown fox", |t, s| split_lines(t, s, 9, LF)),
            "|the quick\nbrown fox"
        );
    }
}
