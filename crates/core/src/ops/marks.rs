//! Search > Bookmark operations on bookmarked lines, and Search > Copy Styled Text.
//!
//! Lines are given as sorted line numbers (from `LineMarkers::lines`); every edit is one
//! transaction.

use std::ops::Range;

use ropey::Rope;

use crate::change::Edit;
use crate::line_ending::LineEnding;
use crate::motion::{line_count, line_range, line_range_with_break};
use crate::transaction::Transaction;

/// Copy Bookmarked Lines: the text of `lines`, each followed by `eol`.
pub fn copy_lines(text: &Rope, lines: &[usize], eol: LineEnding) -> String {
    let count = line_count(text);
    let mut out = String::new();
    for &line in lines.iter().filter(|&&line| line < count) {
        out.extend(text.slice(line_range(text, line)).chunks());
        out.push_str(eol.as_str());
    }
    out
}

/// Copy Styled Text: the text of each range, in document order, each followed by `eol`.
/// The same range found in several styles is copied once.
pub fn copy_ranges(
    text: &Rope,
    ranges: impl IntoIterator<Item = Range<usize>>,
    eol: LineEnding,
) -> String {
    let mut ranges: Vec<Range<usize>> = ranges.into_iter().collect();
    ranges.sort_by_key(|range| (range.start, range.end));
    ranges.dedup();
    let mut out = String::new();
    for range in ranges {
        out.extend(text.slice(range).chunks());
        out.push_str(eol.as_str());
    }
    out
}

/// Remove Bookmarked Lines (and Cut Bookmarked Lines): deletes `lines` with their line
/// breaks. A block that reaches the last line takes the line break before it instead, so no
/// empty line is left behind (as Delete Current Line does). `None` if there is nothing to
/// delete.
pub fn remove_lines(text: &Rope, lines: &[usize]) -> Option<Transaction> {
    let count = line_count(text);
    let mut edits = Vec::new();
    for block in blocks(lines, count) {
        let start = line_range_with_break(text, block.start).start;
        let end = line_range_with_break(text, block.end - 1).end;
        let start = if block.end == count && block.start > 0 {
            line_range(text, block.start - 1).end
        } else {
            start
        };
        if start < end {
            edits.push(Edit::delete(start..end));
        }
    }
    if edits.is_empty() {
        return None;
    }
    Transaction::from_edits(text, edits).ok()
}

/// Remove Unmarked Lines: deletes every line that is not in `lines`.
pub fn remove_other_lines(text: &Rope, lines: &[usize]) -> Option<Transaction> {
    remove_lines(text, &other_lines(line_count(text), lines))
}

/// Paste to (Replace) Bookmarked Lines: replaces the content of each of `lines` (not its line
/// break) with `with`. `None` if nothing changes.
pub fn replace_lines(text: &Rope, lines: &[usize], with: &str) -> Option<Transaction> {
    let count = line_count(text);
    let mut last = None;
    let edits: Vec<Edit> = lines
        .iter()
        .copied()
        .filter(|&line| {
            let keep = line < count && last.is_none_or(|last| line > last);
            last = Some(line);
            keep
        })
        .map(|line| line_range(text, line))
        .filter(|range| text.slice(range.clone()) != with)
        .map(|range| Edit::replace(range, with))
        .collect();
    if edits.is_empty() {
        return None;
    }
    Transaction::from_edits(text, edits).ok()
}

/// Inverse Bookmark: the lines among `0..line_count` that are not in `lines`.
pub fn other_lines(line_count: usize, lines: &[usize]) -> Vec<usize> {
    let mut marked = lines.iter().copied().peekable();
    (0..line_count)
        .filter(|&line| {
            while marked.next_if(|&marked| marked < line).is_some() {}
            marked.next_if_eq(&line).is_none()
        })
        .collect()
}

/// Sorted line numbers grouped into runs of consecutive lines.
fn blocks(lines: &[usize], count: usize) -> Vec<Range<usize>> {
    let mut blocks: Vec<Range<usize>> = Vec::new();
    for &line in lines.iter().filter(|&&line| line < count) {
        match blocks.last_mut() {
            Some(last) if line <= last.end => last.end = last.end.max(line + 1),
            _ => blocks.push(line..line + 1),
        }
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(text: &str, transaction: Option<Transaction>) -> String {
        let mut rope = Rope::from_str(text);
        if let Some(transaction) = transaction {
            transaction.changes().apply(&mut rope);
        }
        rope.to_string()
    }

    #[test]
    fn copies_lines_and_ranges_one_per_line() {
        let text = Rope::from_str("one\r\ntwo\nthree");
        assert_eq!(copy_lines(&text, &[0, 2], LineEnding::Lf), "one\nthree\n");
        assert_eq!(copy_lines(&text, &[], LineEnding::Lf), "");
        assert_eq!(
            copy_ranges(&text, [9..14, 0..3, 9..14, 1..2], LineEnding::CrLf),
            "one\r\nn\r\nthree\r\n"
        );
    }

    #[test]
    fn removes_bookmarked_lines_without_leaving_an_empty_line() {
        let text = "0\n1\n2\n3\n4";
        assert_eq!(
            apply(text, remove_lines(&Rope::from_str(text), &[1, 2, 4])),
            "0\n3"
        );
        assert_eq!(
            apply(text, remove_lines(&Rope::from_str(text), &[0])),
            "1\n2\n3\n4"
        );
        assert_eq!(
            apply(text, remove_lines(&Rope::from_str(text), &[0, 1, 2, 3, 4])),
            ""
        );
        assert!(remove_lines(&Rope::from_str(text), &[]).is_none());
        // The empty line after a final line break is a line too.
        assert_eq!(
            apply("a\nb\n", remove_lines(&Rope::from_str("a\nb\n"), &[2])),
            "a\nb"
        );
        assert_eq!(
            apply(
                "a\r\nb\r\n",
                remove_lines(&Rope::from_str("a\r\nb\r\n"), &[0])
            ),
            "b\r\n"
        );
    }

    #[test]
    fn removes_unmarked_lines() {
        let text = "0\n1\n2\n3\n4";
        assert_eq!(
            apply(text, remove_other_lines(&Rope::from_str(text), &[1, 3])),
            "1\n3"
        );
        assert_eq!(
            apply(text, remove_other_lines(&Rope::from_str(text), &[4])),
            "4"
        );
        assert!(remove_other_lines(&Rope::from_str(text), &[0, 1, 2, 3, 4]).is_none());
    }

    #[test]
    fn pastes_over_bookmarked_lines() {
        let text = "a\nb\n\nc";
        assert_eq!(
            apply(text, replace_lines(&Rope::from_str(text), &[0, 2, 3], "X")),
            "X\nb\nX\nX"
        );
        assert_eq!(
            apply(text, replace_lines(&Rope::from_str(text), &[1], "1\n2")),
            "a\n1\n2\n\nc"
        );
        assert!(replace_lines(&Rope::from_str(text), &[1], "b").is_none());
    }

    #[test]
    fn inverse_takes_the_other_lines() {
        assert_eq!(other_lines(5, &[1, 3]), [0, 2, 4]);
        assert_eq!(other_lines(3, &[]), [0, 1, 2]);
        assert_eq!(other_lines(2, &[0, 1, 7]), Vec::<usize>::new());
    }
}
