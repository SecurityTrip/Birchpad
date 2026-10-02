//! Mapping between document lines and visual rows.
//!
//! Without word wrap every line is one row. With word wrap a line takes as many rows as it needs
//! at the current width; the map keeps a row count per line and prefix sums over them, so that
//! row ↔ line conversions are O(log n). Lines hidden by collapsed folds have no rows: without
//! word wrap, rows skip them through prefix sums over the hidden ranges; with it, they count
//! zero rows in the prefix sums.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use birchpad_core::motion::{line_count, line_of, line_range};
use birchpad_core::{ChangeSet, LINE_TYPE, Operation, Rope};

use crate::cells::{cells_at, pos_at_column};
use crate::wrap::{count_rows, row_count, wrap_line};

/// Lines longer than this (in bytes) get a column index, so that horizontal positions deep in a
/// multi-megabyte line are found without scanning it from the start every frame.
const LONG_LINE: usize = 8 * 1024;
const CHECKPOINT: usize = 4 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayoutConfig {
    pub tab_width: usize,
    /// Wrap width in cells; `None` disables word wrap.
    pub wrap_width: Option<usize>,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            tab_width: 4,
            wrap_width: None,
        }
    }
}

/// One visual row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub line: usize,
    /// 0 for the first row of a line, 1 for the next wrapped part, ...
    pub index_in_line: usize,
    /// Document bytes shown in the row, without the line break.
    pub range: Range<usize>,
    /// Column of `range.start` counted from the start of the line (tab stops are per line).
    pub start_column: usize,
    pub last_in_line: bool,
}

/// Positions of a long line at regular byte intervals, with their columns.
#[derive(Debug)]
struct ColumnIndex {
    points: Vec<(usize, usize)>,
}

impl ColumnIndex {
    fn build(text: &Rope, range: Range<usize>, tab_width: usize) -> Self {
        let mut points = vec![(range.start, 0)];
        let (mut pos, mut column) = (range.start, 0);
        for ch in text.slice(range).chars() {
            if pos - points.last().expect("starts non-empty").0 >= CHECKPOINT {
                points.push((pos, column));
            }
            column += cells_at(ch, column, tab_width);
            pos += ch.len_utf8();
        }
        Self { points }
    }

    fn before_pos(&self, pos: usize) -> (usize, usize) {
        let index = self.points.partition_point(|&(p, _)| p <= pos);
        self.points[index.saturating_sub(1)]
    }

    fn before_column(&self, column: usize) -> (usize, usize) {
        let index = self.points.partition_point(|&(_, c)| c <= column);
        self.points[index.saturating_sub(1)]
    }
}

#[derive(Debug, Default)]
pub struct DisplayMap {
    config: LayoutConfig,
    /// Rows of each line while wrapping; empty otherwise.
    rows_per_line: Vec<u32>,
    /// `rows_before[i]` is the number of rows in lines `0..i`; valid up to `valid_prefix`.
    rows_before: Vec<usize>,
    valid_prefix: usize,
    wraps: HashMap<usize, Arc<Vec<(usize, usize)>>>,
    columns: HashMap<usize, Arc<ColumnIndex>>,
    /// Lines hidden by collapsed folds: sorted, disjoint.
    hidden: Vec<Range<usize>>,
    /// `hidden_before[i]` is the number of lines hidden by `hidden[..i]`.
    hidden_before: Vec<usize>,
}

impl DisplayMap {
    pub fn new(text: &Rope, config: LayoutConfig) -> Self {
        let mut map = Self {
            config,
            ..Self::default()
        };
        map.reset(text);
        map
    }

    pub fn config(&self) -> LayoutConfig {
        self.config
    }

    /// Changes tab width or wrap width, relaying out if needed.
    pub fn set_config(&mut self, text: &Rope, config: LayoutConfig) {
        if config != self.config {
            self.config = config;
            self.reset(text);
        }
    }

    pub fn is_wrapping(&self) -> bool {
        self.config.wrap_width.is_some()
    }

    /// Lays out the whole text from scratch (after loading or replacing it).
    pub fn reset(&mut self, text: &Rope) {
        let rows = Self::compute_rows(text, self.config);
        self.install(self.config, rows);
    }

    /// Rows per line of `text` laid out with `config` (empty without word wrap). Pure and
    /// `Send`, so large documents can be laid out on a background thread and the result handed
    /// to [`Self::install`].
    pub fn compute_rows(text: &Rope, config: LayoutConfig) -> Vec<u32> {
        let Some(width) = config.wrap_width else {
            return Vec::new();
        };
        let mut rows = Vec::with_capacity(line_count(text));
        for line in text.lines(LINE_TYPE) {
            let content = line
                .trailing_line_break_idx(LINE_TYPE)
                .unwrap_or(line.len());
            rows.push(count_rows(line.slice(..content), width, config.tab_width) as u32);
        }
        // `lines()` yields nothing for an empty text but there is one (empty) line.
        while rows.len() < line_count(text) {
            rows.push(1);
        }
        rows
    }

    /// Switches to `config` with rows computed by [`Self::compute_rows`] for the current text.
    pub fn install(&mut self, config: LayoutConfig, rows: Vec<u32>) {
        self.config = config;
        self.wraps.clear();
        self.columns.clear();
        self.rows_per_line = rows;
        self.valid_prefix = 0;
    }

    /// Updates the layout after `changes` produced `text`.
    pub fn edit(&mut self, text: &Rope, changes: &ChangeSet) {
        self.wraps.clear();
        self.columns.clear();
        let Some(width) = self.config.wrap_width else {
            return;
        };
        let Some(changed) = changed_range(changes) else {
            return;
        };
        let new_lines = line_count(text);
        let old_lines = self.rows_per_line.len();
        // One line of margin on each side: an edit next to a line break can merge "\r" + "\n".
        let first = line_of(text, changed.start).saturating_sub(1);
        let last = (line_of(text, changed.end) + 1).min(new_lines - 1);
        let delta = new_lines as isize - old_lines as isize;
        let old_last = (last as isize - delta).max(first as isize - 1);
        let old_end = (old_last + 1).clamp(first as isize, old_lines as isize) as usize;
        let rows: Vec<u32> = (first..=last)
            .map(|line| {
                row_count(text, line_range(text, line), width, self.config.tab_width) as u32
            })
            .collect();
        self.rows_per_line.splice(first..old_end, rows);
        self.valid_prefix = self.valid_prefix.min(first);
        debug_assert_eq!(self.rows_per_line.len(), new_lines);
    }

    /// Rows per line while wrapping (for tests and diagnostics).
    pub fn rows_per_line(&self) -> &[u32] {
        &self.rows_per_line
    }

    /// Hides `ranges` of lines (sorted, disjoint), as collapsed folds do.
    pub fn set_hidden(&mut self, ranges: Vec<Range<usize>>) {
        if ranges == self.hidden {
            return;
        }
        let first_change = self
            .hidden
            .iter()
            .zip(&ranges)
            .position(|(old, new)| old != new)
            .unwrap_or_else(|| self.hidden.len().min(ranges.len()));
        let changed_line = [self.hidden.get(first_change), ranges.get(first_change)]
            .into_iter()
            .flatten()
            .map(|range| range.start)
            .min()
            .unwrap_or(0);
        self.hidden = ranges;
        self.hidden_before = std::iter::once(0)
            .chain(self.hidden.iter().scan(0, |total, range| {
                *total += range.len();
                Some(*total)
            }))
            .collect();
        self.valid_prefix = self.valid_prefix.min(changed_line);
    }

    pub fn hidden(&self) -> &[Range<usize>] {
        &self.hidden
    }

    /// The hidden range containing `line`, if it is hidden.
    fn hidden_range(&self, line: usize) -> Option<&Range<usize>> {
        let index = self.hidden.partition_point(|range| range.end <= line);
        self.hidden.get(index).filter(|range| range.start <= line)
    }

    pub fn is_hidden(&self, line: usize) -> bool {
        self.hidden_range(line).is_some()
    }

    /// `line`, or the visible line above it if it is hidden (the fold's header).
    pub fn visible_line(&self, line: usize) -> usize {
        self.hidden_range(line)
            .map_or(line, |range| range.start.saturating_sub(1))
    }

    /// Hidden lines before `line` (which is visible), without word wrap.
    fn hidden_lines_before(&self, line: usize) -> usize {
        let index = self.hidden.partition_point(|range| range.start < line);
        self.hidden_before.get(index).copied().unwrap_or(0)
    }

    /// Lines hidden in a text of `lines` lines (ranges past its end are ignored).
    fn hidden_total(&self, lines: usize) -> usize {
        self.hidden
            .iter()
            .map(|range| range.end.min(lines).saturating_sub(range.start))
            .sum()
    }

    /// The line shown in unwrapped row `row`.
    fn line_of_unwrapped_row(&self, row: usize) -> usize {
        let count = self.partition_hidden(row);
        row + self.hidden_before.get(count).copied().unwrap_or(0)
    }

    /// Number of hidden ranges that start at or before the line shown in row `row`.
    fn partition_hidden(&self, row: usize) -> usize {
        let (mut low, mut high) = (0, self.hidden.len());
        while low < high {
            let middle = (low + high) / 2;
            // Visible lines before this range starts.
            if self.hidden[middle].start - self.hidden_before[middle] <= row {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        low
    }

    fn ensure_prefix(&mut self, upto: usize) {
        if self.rows_before.len() != self.rows_per_line.len() + 1 {
            self.rows_before.resize(self.rows_per_line.len() + 1, 0);
            self.valid_prefix = self.valid_prefix.min(self.rows_per_line.len());
        }
        let upto = upto.min(self.rows_per_line.len());
        while self.valid_prefix < upto {
            let i = self.valid_prefix;
            let rows = if self.is_hidden(i) {
                0
            } else {
                self.rows_per_line[i] as usize
            };
            self.rows_before[i + 1] = self.rows_before[i] + rows;
            self.valid_prefix += 1;
        }
    }

    pub fn row_count(&mut self, text: &Rope) -> usize {
        if self.is_wrapping() {
            let lines = self.rows_per_line.len();
            self.ensure_prefix(lines);
            self.rows_before[lines]
        } else {
            let lines = line_count(text);
            lines - self.hidden_total(lines)
        }
    }

    /// The first row of `line`; for a hidden line, of the visible line above it.
    pub fn first_row_of_line(&mut self, line: usize) -> usize {
        let line = self.visible_line(line);
        if self.is_wrapping() {
            self.ensure_prefix(line);
            self.rows_before[line.min(self.rows_per_line.len())]
        } else {
            line - self.hidden_lines_before(line)
        }
    }

    /// Row starts of a line (offset and column), wrapped or not.
    fn row_starts(&mut self, text: &Rope, line: usize) -> Arc<Vec<(usize, usize)>> {
        let range = line_range(text, line);
        match self.config.wrap_width {
            None => Arc::new(vec![(range.start, 0)]),
            Some(width) => self
                .wraps
                .entry(line)
                .or_insert_with(|| Arc::new(wrap_line(text, range, width, self.config.tab_width)))
                .clone(),
        }
    }

    fn make_row(&mut self, text: &Rope, line: usize, index: usize) -> Row {
        let starts = self.row_starts(text, line);
        let index = index.min(starts.len() - 1);
        let (start, start_column) = starts[index];
        let end = starts
            .get(index + 1)
            .map_or_else(|| line_range(text, line).end, |&(next, _)| next);
        Row {
            line,
            index_in_line: index,
            range: start..end,
            start_column,
            last_in_line: index + 1 == starts.len(),
        }
    }

    /// The row with index `row` (clamped to the last row).
    pub fn row(&mut self, text: &Rope, row: usize) -> Row {
        if !self.is_wrapping() {
            let row = row.min(self.row_count(text).saturating_sub(1));
            let line = self.line_of_unwrapped_row(row).min(line_count(text) - 1);
            return self.make_row(text, line, 0);
        }
        let lines = self.rows_per_line.len();
        self.ensure_prefix(lines);
        let total = self.rows_before[lines];
        let row = row.min(total.saturating_sub(1));
        // The last line whose first row is at or before `row`.
        let line = self.rows_before[..=lines]
            .partition_point(|&before| before <= row)
            .saturating_sub(1)
            .min(lines - 1);
        let index = row - self.rows_before[line];
        self.make_row(text, line, index)
    }

    /// The row index and row containing `pos`. A position where a wrapped line breaks belongs
    /// to the row that starts there.
    /// A position in a hidden line belongs to the last row of the fold's header.
    pub fn row_of(&mut self, text: &Rope, pos: usize) -> (usize, Row) {
        let line = line_of(text, pos);
        let visible = self.visible_line(line);
        let pos = if visible == line {
            pos
        } else {
            line_range(text, visible).end
        };
        let line = visible;
        let starts = self.row_starts(text, line);
        let index = starts
            .partition_point(|&(start, _)| start <= pos)
            .saturating_sub(1);
        let first = self.first_row_of_line(line);
        (first + index, self.make_row(text, line, index))
    }

    /// Column of `pos`, counted from the start of its line.
    pub fn column(&mut self, text: &Rope, pos: usize) -> usize {
        let line = line_of(text, pos);
        let range = line_range(text, line);
        let pos = pos.min(range.end);
        let (from, column) = if range.len() > LONG_LINE {
            self.column_index(text, line).before_pos(pos)
        } else {
            (range.start, 0)
        };
        crate::cells::column_after(text, from, column, pos, self.config.tab_width)
    }

    /// The position in `row` closest to `column` (counted from the start of the line).
    pub fn pos_at_column(&mut self, text: &Rope, row: &Row, column: usize) -> usize {
        let (from, from_column) = if row.range.len() > LONG_LINE {
            let (pos, at) = self.column_index(text, row.line).before_column(column);
            if pos >= row.range.start {
                (pos, at)
            } else {
                (row.range.start, row.start_column)
            }
        } else {
            (row.range.start, row.start_column)
        };
        let (pos, _) = pos_at_column(
            text,
            from..row.range.end,
            from_column,
            column,
            self.config.tab_width,
        );
        // A wrapped row's end is the next row's start: stay on this row.
        if !row.last_in_line && pos == row.range.end && pos > row.range.start {
            birchpad_core::motion::prev_boundary(text, pos)
        } else {
            pos
        }
    }

    /// The position at `column` of `line`, ignoring word wrap, and the virtual space past the
    /// line's end if the line is shorter. A column inside a character rounds to the nearer
    /// side of it, as clicking there does.
    pub fn pos_at_line_column(
        &mut self,
        text: &Rope,
        line: usize,
        column: usize,
    ) -> (usize, usize) {
        let range = line_range(text, line);
        let (from, from_column) = if range.len() > LONG_LINE {
            self.column_index(text, line).before_column(column)
        } else {
            (range.start, 0)
        };
        let (pos, reached) = pos_at_column(
            text,
            from..range.end,
            from_column,
            column,
            self.config.tab_width,
        );
        let virtual_cells = if pos == range.end {
            column.saturating_sub(reached)
        } else {
            0
        };
        (pos, virtual_cells)
    }

    fn column_index(&mut self, text: &Rope, line: usize) -> Arc<ColumnIndex> {
        let tab_width = self.config.tab_width;
        self.columns
            .entry(line)
            .or_insert_with(|| {
                Arc::new(ColumnIndex::build(text, line_range(text, line), tab_width))
            })
            .clone()
    }

    /// Moves `pos` by `rows` visual rows, aiming for `goal` columns from the start of the row
    /// (the column the caret had when vertical movement began). Returns the new position.
    pub fn move_by_rows(&mut self, text: &Rope, pos: usize, rows: isize, goal: usize) -> usize {
        let (current, _) = self.row_of(text, pos);
        let target = current as isize + rows;
        if target < 0 {
            return 0;
        }
        if target as usize >= self.row_count(text) {
            return text.len();
        }
        let row = self.row(text, target as usize);
        self.pos_at_column(text, &row, row.start_column + goal)
    }

    /// Columns from the start of its row to `pos` (the goal for vertical movement).
    pub fn column_in_row(&mut self, text: &Rope, pos: usize) -> usize {
        let (_, row) = self.row_of(text, pos);
        self.column(text, pos) - row.start_column
    }
}

/// The part of the new text that `changes` touched: from the first to the end of the last edit.
pub(crate) fn changed_range(changes: &ChangeSet) -> Option<Range<usize>> {
    let mut new_pos = 0;
    let mut range: Option<Range<usize>> = None;
    for op in changes.ops() {
        match op {
            Operation::Retain(n) => new_pos += n,
            Operation::Delete(_) => {
                range.get_or_insert(new_pos..new_pos).end = new_pos;
            }
            Operation::Insert(s) => {
                let start = new_pos;
                new_pos += s.len();
                range.get_or_insert(start..start).end = new_pos;
            }
        }
    }
    range
}

#[cfg(test)]
mod tests {
    use super::*;
    use birchpad_core::Edit;

    fn wrapped(width: usize) -> LayoutConfig {
        LayoutConfig {
            tab_width: 4,
            wrap_width: Some(width),
        }
    }

    #[test]
    fn unwrapped_rows_are_lines() {
        let text = Rope::from_str("one\ntwo\r\nthree");
        let mut map = DisplayMap::new(&text, LayoutConfig::default());
        assert_eq!(map.row_count(&text), 3);
        let row = map.row(&text, 1);
        assert_eq!((row.line, row.range.clone()), (1, 4..7));
        assert_eq!(map.row_of(&text, 9).0, 2);
        assert_eq!(map.row(&text, 99).line, 2, "clamped");
    }

    #[test]
    fn wrapped_rows_map_both_ways() {
        let text = Rope::from_str("aaaa bbbb cccc\nshort\ndddd eeee");
        let mut map = DisplayMap::new(&text, wrapped(5));
        assert_eq!(map.rows_per_line(), [3, 1, 2]);
        assert_eq!(map.row_count(&text), 6);
        assert_eq!(map.first_row_of_line(2), 4);
        let row = map.row(&text, 1);
        assert_eq!(
            (row.line, row.index_in_line, row.range.clone()),
            (0, 1, 5..10)
        );
        assert_eq!(map.row(&text, 3).line, 1);
        let last = map.row(&text, 5);
        assert_eq!(
            (last.line, last.index_in_line, last.last_in_line),
            (2, 1, true)
        );

        // The break position belongs to the next row.
        assert_eq!(map.row_of(&text, 5).0, 1);
        assert_eq!(map.row_of(&text, 4).0, 0);
    }

    #[test]
    fn vertical_movement_keeps_the_goal_column() {
        let text = Rope::from_str("aaaa bbbb cccc\nxy\ndddd eeee");
        let mut map = DisplayMap::new(&text, wrapped(5));
        // From column 3 of row 0 ("aaaa ") down to row 1 ("bbbb "), then row 2 ("cccc").
        let pos = map.move_by_rows(&text, 3, 1, 3);
        assert_eq!(pos, 8);
        let pos = map.move_by_rows(&text, pos, 1, 3);
        assert_eq!(pos, 13);
        // "xy" is shorter: the end of the line.
        assert_eq!(map.move_by_rows(&text, pos, 1, 3), 17);
        // Within a wrapped row the caret never lands on the next row's start.
        let row = map.row(&text, 0);
        assert_eq!(map.pos_at_column(&text, &row, 99), 4);
        assert_eq!(map.move_by_rows(&text, 0, -1, 3), 0);
        assert_eq!(map.move_by_rows(&text, 0, 99, 3), text.len());
    }

    #[test]
    fn columns_in_long_lines_use_the_index() {
        let line = "a\tb".repeat(10_000);
        let text = Rope::from_str(&line);
        let mut map = DisplayMap::new(&text, LayoutConfig::default());
        // The first "a\tb" takes 5 columns, every later one 4 (its tab ends on a stop), so
        // unit k starts at column 4k + 1.
        assert_eq!(map.column(&text, 3 * 5_000), 4 * 5_000 + 1);
        let row = map.row(&text, 0);
        assert_eq!(map.pos_at_column(&text, &row, 4 * 7_000 + 2), 3 * 7_000 + 1);
        assert_eq!(map.column(&text, text.len()), 4 * 10_000 + 1);
    }

    #[test]
    fn incremental_edits_match_a_fresh_layout() {
        let mut text = Rope::from_str("aaaa bbbb\ncc\r\ndddd eeee ffff\n");
        let mut map = DisplayMap::new(&text, wrapped(5));
        let edits = [
            vec![Edit::insert(2, "\nzz zz zz")],
            vec![Edit::delete(0..12)],
            vec![Edit::insert(3, "\r"), Edit::insert(text.len(), "end")],
            vec![Edit::replace(0..1, "\n\n\n")],
        ];
        for batch in edits {
            let len = text.len();
            let batch: Vec<Edit> = batch
                .into_iter()
                .map(|e| Edit::replace(e.range.start.min(len)..e.range.end.min(len), e.text))
                .collect();
            let changes = ChangeSet::from_edits(&text, batch).unwrap();
            changes.apply(&mut text);
            map.edit(&text, &changes);
            let fresh = DisplayMap::new(&text, wrapped(5));
            assert_eq!(map.rows_per_line(), fresh.rows_per_line(), "{text:?}");
        }
    }
}
