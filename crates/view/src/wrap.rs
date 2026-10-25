//! Word wrap: splitting a line into visual rows that fit a width in cells.

use std::ops::Range;

use birchpad_core::{Rope, RopeSlice};

use crate::cells::cells_at;

/// Where the rows after the first of a wrapped line start.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum WrapIndent {
    /// At the left edge.
    #[default]
    None,
    /// Under the line's indentation.
    Aligned,
    /// A tab stop further in than the line's indentation.
    Indent,
}

/// The cells the rows after the first of `line` (without its line break) start in by, for
/// `mode`, at most half of `width` so that every row keeps room.
pub fn wrap_indent(line: RopeSlice, width: usize, tab_width: usize, mode: WrapIndent) -> usize {
    if mode == WrapIndent::None {
        return 0;
    }
    let mut column = 0;
    for ch in line.chars() {
        if ch != ' ' && ch != '\t' {
            break;
        }
        column += cells_at(ch, column, tab_width);
    }
    if mode == WrapIndent::Indent {
        column += tab_width.max(1);
    }
    column.min(width / 2)
}

/// Word-wrap state machine: feed characters, get row starts.
struct Wrapper {
    width: usize,
    /// Cells the rows after the first lose to their indentation.
    indent: usize,
    tab_width: usize,
    row_start: usize,
    row_start_column: usize,
    column: usize,
    pos: usize,
    /// The latest place this row may break: (offset, column).
    opportunity: Option<(usize, usize)>,
}

impl Wrapper {
    /// Starts wrapping at a row start: `start`, at `column` from the line start. Starting again
    /// at any row start a line wrapped into gives the same rows after it.
    fn new(start: usize, column: usize, width: usize, tab_width: usize, indent: usize) -> Self {
        Self {
            width: width.max(1),
            indent,
            tab_width,
            row_start: start,
            row_start_column: column,
            column,
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
        // Rows after the first start further in: they hold less.
        let width = if self.row_start_column == 0 {
            self.width
        } else {
            self.width.saturating_sub(self.indent).max(1)
        };
        if !whitespace
            && self.column + cells - self.row_start_column > width
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
pub(crate) fn count_rows(line: RopeSlice, width: usize, tab_width: usize, indent: usize) -> usize {
    if line.len() <= width && !line.bytes().any(|b| b == b'\t') {
        return 1;
    }
    let mut wrapper = Wrapper::new(0, 0, width, tab_width, indent);
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
    let mut wrapper = Wrapper::new(range.start, 0, width, tab_width, 0);
    let mut rows = vec![(range.start, 0)];
    rows.extend(text.slice(range).chars().filter_map(|ch| wrapper.push(ch)));
    rows
}

/// Rows of `line` (its range in `text`) as offsets from its start and columns.
pub(crate) fn line_rows(
    text: &Rope,
    line: Range<usize>,
    width: usize,
    tab_width: usize,
    indent: usize,
) -> Vec<(usize, usize)> {
    let mut rows = vec![(0, 0)];
    rows.extend(rows_after(text, line, (0, 0), width, tab_width, indent));
    rows
}

/// The rows of `line` after the one that starts at `from` (offset from the line start, column).
fn rows_after(
    text: &Rope,
    line: Range<usize>,
    (offset, column): (usize, usize),
    width: usize,
    tab_width: usize,
    indent: usize,
) -> impl Iterator<Item = (usize, usize)> {
    let mut wrapper = Wrapper::new(line.start + offset, column, width, tab_width, indent);
    text.slice(line.start + offset..line.end)
        .chars()
        .filter_map(move |ch| wrapper.push(ch))
        .map(move |(pos, column)| (pos - line.start, column))
}

/// The rows of a line after an edit, from its rows before it (offsets and columns, as from
/// [`line_rows`]). The edit replaced the offsets `old` with text that ends at `new_end`, which
/// made the line `line` of `text`.
///
/// Wrapping starts again one row before the edit (where a row ends depends on the character
/// that did not fit, at the start of the next row) and stops at the first row after the edit
/// that starts where a row started before: later rows are the same, moved. A shift of the
/// columns by part of a tab stop changes nothing until the next tab; past it, the columns
/// differ by whole tab stops again.
pub(crate) fn rewrap(
    text: &Rope,
    line: Range<usize>,
    rows: &[(usize, usize)],
    old: Range<usize>,
    new_end: usize,
    width: usize,
    tab_width: usize,
) -> Vec<(usize, usize)> {
    let delta = new_end as isize - old.end as isize;
    let stop = tab_width.max(1) as isize;
    let moved = |(offset, column): (usize, usize), columns: isize| {
        (
            offset.strict_add_signed(delta),
            column.strict_add_signed(columns),
        )
    };
    let restart = rows
        .partition_point(|&(offset, _)| offset <= old.start)
        .saturating_sub(2);
    let mut wrapped = rows[..=restart].to_vec();
    let mut from = rows[restart];
    // Rows starting before this may still change.
    let mut settled = new_end;
    'wrapping: loop {
        for (offset, column) in rows_after(text, line.clone(), from, width, tab_width, 0) {
            wrapped.push((offset, column));
            if offset < settled {
                continue;
            }
            let before = offset.strict_add_signed(-delta);
            let Ok(same) = rows.binary_search_by_key(&before, |&(offset, _)| offset) else {
                continue;
            };
            let columns = column as isize - rows[same].1 as isize;
            let tab = if columns % stop == 0 {
                None
            } else {
                next_tab(text, line.start + offset..line.end).map(|tab| tab - line.start)
            };
            let Some(tab) = tab else {
                wrapped.extend(rows[same + 1..].iter().map(|&row| moved(row, columns)));
                return wrapped;
            };
            // The rows up to the one before the last that starts at or before the tab are the
            // same; wrapping goes on from there.
            let up_to_tab = same
                + rows[same..]
                    .partition_point(|&(offset, _)| offset.strict_add_signed(delta) <= tab);
            let restart = (up_to_tab - 1).saturating_sub(1).max(same);
            wrapped.extend(
                rows[same + 1..=restart]
                    .iter()
                    .map(|&row| moved(row, columns)),
            );
            from = moved(rows[restart], columns);
            settled = tab + 1;
            continue 'wrapping;
        }
        return wrapped;
    }
}

/// The position of the first tab in `range`.
pub(crate) fn next_tab(text: &Rope, range: Range<usize>) -> Option<usize> {
    let mut pos = range.start;
    for chunk in text.slice(range).chunks() {
        if let Some(index) = chunk.find('\t') {
            return Some(pos + index);
        }
        pos += chunk.len();
    }
    None
}

/// Number of rows `range` wraps into; fast for lines that obviously fit.
///
/// Characters never take more cells than bytes (wide ones are 3-4 bytes in UTF-8), so a line
/// without tabs and no longer than the width in bytes fits in one row.
pub fn row_count(text: &Rope, range: Range<usize>, width: usize, tab_width: usize) -> usize {
    count_rows(text.slice(range), width, tab_width, 0)
}
