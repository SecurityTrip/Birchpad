//! Word wrap: splitting a line into visual rows that fit a width in cells.

use std::ops::Range;

use birchpad_core::{Rope, RopeSlice};

use crate::cells::cells_at;

/// Word-wrap state machine: feed characters, get row starts.
struct Wrapper {
    width: usize,
    tab_width: usize,
    row_start: usize,
    row_start_column: usize,
    column: usize,
    pos: usize,
    /// The latest place this row may break: (offset, column).
    opportunity: Option<(usize, usize)>,
}

impl Wrapper {
    fn new(start: usize, width: usize, tab_width: usize) -> Self {
        Self {
            width: width.max(1),
            tab_width,
            row_start: start,
            row_start_column: 0,
            column: 0,
            pos: start,
            opportunity: None,
        }
    }

    /// Takes one character; returns the start of a new row if one begins before it.
    #[inline]
    fn push(&mut self, ch: char) -> Option<(usize, usize)> {
        let cells = cells_at(ch, self.column, self.tab_width);
        let wide = cells >= 2 && ch != '\t';
        if wide && self.pos > self.row_start {
            self.opportunity = Some((self.pos, self.column));
        }
        let whitespace = ch == ' ' || ch == '\t';
        let mut new_row = None;
        if !whitespace
            && self.column + cells - self.row_start_column > self.width
            && self.pos > self.row_start
        {
            let (at, at_column) = match self.opportunity {
                Some(found) if found.0 > self.row_start => found,
                _ => (self.pos, self.column),
            };
            self.row_start = at;
            self.row_start_column = at_column;
            self.opportunity = None;
            new_row = Some((at, at_column));
        }
        self.column += cells;
        self.pos += ch.len_utf8();
        if whitespace || wide {
            self.opportunity = Some((self.pos, self.column));
        }
        new_row
    }
}

/// Rows of a line given as a slice (without its line break): counts without allocating.
pub(crate) fn count_rows(line: RopeSlice, width: usize, tab_width: usize) -> usize {
    if line.len() <= width && !line.bytes().any(|b| b == b'\t') {
        return 1;
    }
    let mut wrapper = Wrapper::new(0, width, tab_width);
    1 + line
        .chars()
        .filter(|&ch| wrapper.push(ch).is_some())
        .count()
}

/// Where the rows of a wrapped line start: byte offsets, the first one being `range.start`, and
/// the column (from the line start) of each.
///
/// Rows break after whitespace, or around wide (CJK) characters, like Scintilla's word wrap;
/// a word longer than the width is broken anywhere. Whitespace never starts a row: it hangs past
/// the edge at the end of the previous one. Every row holds at least one character.
pub fn wrap_line(
    text: &Rope,
    range: Range<usize>,
    width: usize,
    tab_width: usize,
) -> Vec<(usize, usize)> {
    let mut wrapper = Wrapper::new(range.start, width, tab_width);
    let mut rows = vec![(range.start, 0)];
    rows.extend(text.slice(range).chars().filter_map(|ch| wrapper.push(ch)));
    rows
}

/// Number of rows `range` wraps into; fast for lines that obviously fit.
///
/// Characters never take more cells than bytes (wide ones are 3-4 bytes in UTF-8), so a line
/// without tabs and no longer than the width in bytes fits in one row.
pub fn row_count(text: &Rope, range: Range<usize>, width: usize, tab_width: usize) -> usize {
    count_rows(text.slice(range), width, tab_width)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(text: &str, width: usize) -> Vec<String> {
        let rope = Rope::from_str(text);
        let starts = wrap_line(&rope, 0..rope.len(), width, 4);
        let mut out = Vec::new();
        for (i, &(start, _)) in starts.iter().enumerate() {
            let end = starts.get(i + 1).map_or(rope.len(), |&(s, _)| s);
            out.push(rope.slice(start..end).to_string());
        }
        out
    }

    #[test]
    fn breaks_after_spaces() {
        assert_eq!(rows("the quick brown fox", 10), ["the quick ", "brown fox"]);
        assert_eq!(rows("short", 10), ["short"]);
        assert_eq!(rows("", 10), [""]);
    }

    #[test]
    fn breaks_long_words_anywhere() {
        assert_eq!(rows("abcdefghij", 4), ["abcd", "efgh", "ij"]);
        assert_eq!(rows("ab cdefghij", 4), ["ab ", "cdef", "ghij"]);
    }

    #[test]
    fn wide_characters_take_two_cells_and_may_break() {
        assert_eq!(rows("日本語の文章", 5), ["日本", "語の", "文章"]);
        assert_eq!(rows("ab日本", 5), ["ab日", "本"]);
    }

    #[test]
    fn tabs_count_to_their_stop_and_hang() {
        // "\t" takes 4 cells and "a" 1; the second tab hangs at the end of the first row, and
        // "b" starts the second row at column 8.
        assert_eq!(rows("\ta\tb", 5), ["\ta\t", "b"]);
        let rope = Rope::from_str("\ta\tb");
        let starts = wrap_line(&rope, 0..rope.len(), 5, 4);
        assert_eq!(starts, [(0, 0), (3, 8)]);
        assert_eq!(rows("ab      cd", 4), ["ab      ", "cd"]);
    }

    #[test]
    fn row_count_fast_path_agrees() {
        for text in ["", "a b c", "日本語の文章", "\t\tx", "word word word word"] {
            let rope = Rope::from_str(text);
            for width in 1..12 {
                assert_eq!(
                    row_count(&rope, 0..rope.len(), width, 4),
                    wrap_line(&rope, 0..rope.len(), width, 4).len(),
                    "{text:?} at {width}"
                );
            }
        }
    }

    #[test]
    fn every_row_has_a_character_even_when_too_narrow() {
        assert_eq!(rows("日本", 1), ["日", "本"]);
    }
}
