//! Editing operations of Notepad++'s Edit menu as pure functions: given the text and the
//! selection, each returns the transaction to apply (or `None` when there is nothing to do).
//!
//! Every operation is one transaction, so one undo step, whatever the number of carets.

mod blank;
mod case;
mod column;
mod comment;
mod lines;
mod marks;
mod multi;
mod sort;

use std::ops::Range;

use ropey::Rope;

use crate::change::Edit;
use crate::line_ending::LineEnding;
use crate::motion::{line_count, line_of, line_range};
use crate::selection::{Range as Span, Selection};
use crate::transaction::Transaction;

pub use blank::{Trim, eol_to_space, spaces_to_tabs, tabs_to_spaces, trim};
pub use case::{Case, convert_case};
pub use column::{Base, Leading, NumberSequence, format_number, insert_rows, parse_number};
pub use comment::{CommentTokens, block_comment, comment_lines, toggle_comment, uncomment_lines};
pub use lines::{delete_lines, duplicate, insert_blank_line, join_lines, move_lines, split_lines};
pub use marks::{
    copy_lines, copy_ranges, other_lines, remove_lines, remove_other_lines, replace_lines,
};
pub use multi::{MatchOptions, select_all, select_next, skip_to_next};
pub use sort::{
    SortError, SortKey, remove_duplicate_lines, remove_empty_lines, reverse_lines, shuffle_lines,
    sort_lines, sort_lines_by_columns,
};

/// Lines a selection range covers. A non-empty range that ends at the very start of a line does
/// not take that line, as in Scintilla's line operations (selecting whole lines with the
/// mouse ends there).
pub fn lines_of(text: &Rope, range: Span) -> Range<usize> {
    let first = line_of(text, range.from());
    let mut last = line_of(text, range.to());
    if !range.is_empty() && last > first && range.to() == line_range(text, last).start {
        last -= 1;
    }
    first..last + 1
}

/// The lines of every range, merged where they touch, in order.
pub fn line_blocks(text: &Rope, selection: &Selection) -> Vec<Range<usize>> {
    let mut blocks: Vec<Range<usize>> = Vec::new();
    for range in selection.iter() {
        let lines = lines_of(text, *range);
        match blocks.last_mut() {
            Some(last) if lines.start <= last.end => last.end = last.end.max(lines.end),
            _ => blocks.push(lines),
        }
    }
    blocks
}

/// The lines a whole-document operation works on (sorting, trimming...): the selected lines,
/// or every line when nothing is selected.
pub fn target_lines(text: &Rope, selection: &Selection) -> Range<usize> {
    let selected: Vec<Range<usize>> = selection
        .iter()
        .filter(|range| !range.is_empty())
        .map(|range| lines_of(text, *range))
        .collect();
    match (selected.first(), selected.last()) {
        (Some(first), Some(last)) => first.start..last.end,
        _ => 0..line_count(text),
    }
}

/// The contents of `lines`, without line breaks.
pub(crate) fn line_texts(text: &Rope, lines: Range<usize>) -> Vec<String> {
    lines
        .map(|line| text.slice(line_range(text, line)).to_string())
        .collect()
}

/// Bytes from the start of the first line to the end of the last line's content: the line
/// break after the block stays as it is.
pub(crate) fn block_range(text: &Rope, lines: Range<usize>) -> Range<usize> {
    line_range(text, lines.start).start..line_range(text, lines.end - 1).end
}

/// Replaces lines with `new_lines` joined by `eol`, selecting the result if the block was
/// selected, else keeping a caret at the block start. `None` if nothing changes.
pub(crate) fn replace_block(
    text: &Rope,
    selection: &Selection,
    lines: Range<usize>,
    new_lines: &[String],
    eol: LineEnding,
) -> Option<Transaction> {
    let range = block_range(text, lines);
    let replacement = new_lines.join(eol.as_str());
    if text.slice(range.clone()) == replacement.as_str() {
        return None;
    }
    let had_selection = selection.iter().any(|range| !range.is_empty());
    let new_selection = if had_selection {
        Selection::single(Span::new(range.start, range.start + replacement.len()))
    } else {
        Selection::point(range.start)
    };
    Transaction::from_edits(text, [Edit::replace(range, replacement)])
        .ok()
        .map(|transaction| transaction.with_selection(new_selection))
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    /// Text and selection from a string with carets marked `|` and selections `[...]`.
    pub(crate) fn parse(marked: &str) -> (Rope, Selection) {
        let mut text = String::new();
        let mut ranges = Vec::new();
        let mut anchor = None;
        for ch in marked.chars() {
            match ch {
                '|' => ranges.push(Span::point(text.len())),
                '[' => anchor = Some(text.len()),
                ']' => ranges.push(Span::new(anchor.take().unwrap(), text.len())),
                _ => text.push(ch),
            }
        }
        if ranges.is_empty() {
            ranges.push(Span::point(0));
        }
        (Rope::from_str(&text), Selection::new(ranges, 0))
    }

    /// The text after `transaction`, with its selection marked like `parse` reads it.
    pub(crate) fn show(
        text: &Rope,
        selection: &Selection,
        transaction: Option<Transaction>,
    ) -> String {
        let mut text = text.clone();
        let mut selection = selection.clone();
        if let Some(transaction) = transaction {
            transaction.changes().apply(&mut text);
            selection = transaction
                .selection()
                .cloned()
                .unwrap_or_else(|| selection.map(transaction.changes()));
        }
        let mut marks: Vec<(usize, char)> = Vec::new();
        for range in selection.iter() {
            if range.is_empty() {
                marks.push((range.head, '|'));
            } else {
                marks.push((range.from(), '['));
                marks.push((range.to(), ']'));
            }
        }
        marks.sort_by_key(|&(pos, mark)| (pos, mark != ']'));
        let mut out = text.to_string();
        for (pos, mark) in marks.into_iter().rev() {
            out.insert(pos, mark);
        }
        out
    }

    /// Runs `op` on marked text and shows the result.
    pub(crate) fn run(
        marked: &str,
        op: impl FnOnce(&Rope, &Selection) -> Option<Transaction>,
    ) -> String {
        let (text, selection) = parse(marked);
        let transaction = op(&text, &selection);
        show(&text, &selection, transaction)
    }
}
