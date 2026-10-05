//! Mapping between document lines and visual rows.
//!
//! Without word wrap every line is one row. With word wrap a line takes as many rows as it needs
//! at the current width; the map keeps a row count per line and prefix sums over them, so that
//! row ↔ line conversions are O(log n). Lines hidden by collapsed folds have no rows: without
//! word wrap, rows skip them through prefix sums over the hidden ranges; with it, they count
//! zero rows in the prefix sums.
//!
//! The layout of a long line is kept through edits: an edit inside it updates its column index
//! and its rows around the edit, so that typing into a multi-megabyte line costs about as much
//! as typing into a short one.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use birchpad_core::motion::{line_count, line_of, line_range};
use birchpad_core::{ChangeSet, LINE_TYPE, Operation, Rope};

use crate::cells::{cells_at, column_after, pos_at_column};
use crate::wrap::{count_rows, line_rows, rewrap, row_count};

/// Lines longer than this (in bytes) get a column index, so that horizontal positions deep in a
/// multi-megabyte line are found without scanning it from the start every frame, and keep
/// their layout through edits.
#[cfg(not(test))]
const LONG_LINE: usize = 8 * 1024;
#[cfg(not(test))]
const CHECKPOINT: usize = 4 * 1024;
// Small in tests, so that short texts have long lines.
#[cfg(test)]
const LONG_LINE: usize = 12;
#[cfg(test)]
const CHECKPOINT: usize = 5;

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

/// A point of a column index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Checkpoint {
    /// Offset from the line start.
    offset: usize,
    column: usize,
    /// Whether a tab follows before the next checkpoint (or the end of the line).
    tab: bool,
}

/// Offsets of a long line at regular byte intervals, with their columns.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ColumnIndex {
    points: Vec<Checkpoint>,
}

impl ColumnIndex {
    fn build(text: &Rope, line: Range<usize>, tab_width: usize) -> Self {
        let mut index = Self {
            points: vec![Checkpoint {
                offset: 0,
                column: 0,
                tab: false,
            }],
        };
        index.scan(text, line.start, line.len(), tab_width);
        index
    }

    /// Adds checkpoints from the last one up to offset `until`; returns the column there.
    fn scan(&mut self, text: &Rope, line_start: usize, until: usize, tab_width: usize) -> usize {
        let last = self.points.last_mut().expect("starts non-empty");
        last.tab = false;
        let (mut offset, mut column, mut from) = (last.offset, last.column, last.offset);
        for ch in text.slice(line_start + offset..line_start + until).chars() {
            if offset - from >= CHECKPOINT {
                self.points.push(Checkpoint {
                    offset,
                    column,
                    tab: false,
                });
                from = offset;
            }
            if ch == '\t' {
                self.points.last_mut().expect("starts non-empty").tab = true;
            }
            column += cells_at(ch, column, tab_width);
            offset += ch.len_utf8();
        }
        column
    }

    /// Updates the index after an edit replaced the offsets `old` with text ending at offset
    /// `new_end`. `line` is the line's new range in `text`.
    ///
    /// Checkpoints before the edit stay. The text from the last of them to the first checkpoint
    /// after the edit is scanned again, which gives how the edit shifted the columns; later
    /// checkpoints shift as much, up to the first tab after them if the shift is not a whole
    /// tab stop (past a tab, it is).
    fn edit(
        &mut self,
        text: &Rope,
        line: Range<usize>,
        old: Range<usize>,
        new_end: usize,
        tab_width: usize,
    ) {
        let delta = new_end as isize - old.end as isize;
        let keep = self
            .points
            .partition_point(|point| point.offset <= old.start);
        let after = self
            .points
            .partition_point(|point| point.offset < old.end)
            .max(keep);
        let later = self.points.split_off(after);
        self.points.truncate(keep);
        let Some(first) = later.first() else {
            self.scan(text, line.start, line.len(), tab_width);
            return;
        };
        let column = self.scan(
            text,
            line.start,
            first.offset.strict_add_signed(delta),
            tab_width,
        );
        let stop = tab_width.max(1) as isize;
        let mut shift = column as isize - first.column as isize;
        for (index, point) in later.iter().enumerate() {
            let offset = point.offset.strict_add_signed(delta);
            let column = point.column.strict_add_signed(shift);
            let last = self.points.last_mut().expect("starts non-empty");
            if last.offset == offset {
                // The edit deleted everything between them.
                last.tab = point.tab;
            } else {
                self.points.push(Checkpoint {
                    offset,
                    column,
                    tab: point.tab,
                });
            }
            if point.tab
                && shift % stop != 0
                && let Some(next) = later.get(index + 1)
            {
                let next_offset = next.offset.strict_add_signed(delta);
                let next_column = column_after(
                    text,
                    line.start + offset,
                    column,
                    line.start + next_offset,
                    tab_width,
                );
                shift = next_column as isize - next.column as isize;
            }
        }
    }

    /// The last checkpoint at or before `offset`: its offset and column.
    fn before_offset(&self, offset: usize) -> (usize, usize) {
        let index = self.points.partition_point(|point| point.offset <= offset);
        let point = self.points[index.saturating_sub(1)];
        (point.offset, point.column)
    }

    /// The last checkpoint at or before `column`: its offset and column.
    fn before_column(&self, column: usize) -> (usize, usize) {
        let index = self.points.partition_point(|point| point.column <= column);
        let point = self.points[index.saturating_sub(1)];
        (point.offset, point.column)
    }
}

/// What is kept of the layout of a line.
#[derive(Debug)]
struct LineLayout {
    /// The line's range in the text, without its line break.
    range: Range<usize>,
    /// With word wrap, where its rows start: offsets from the line start, and columns.
    rows: Option<Arc<Vec<(usize, usize)>>>,
    /// For a long line, its column index.
    columns: Option<ColumnIndex>,
}

impl LineLayout {
    fn new(range: Range<usize>) -> Self {
        Self {
            range,
            rows: None,
            columns: None,
        }
    }
}

/// A replacement a change set makes: the range of the old text and the range of the new text
/// that replaces it.
#[derive(Debug)]
struct Replacement {
    old: Range<usize>,
    new: Range<usize>,
    /// Whether the new text has line breaks.
    breaks: bool,
}

fn replacements(changes: &ChangeSet) -> Vec<Replacement> {
    let (mut old, mut new) = (0, 0);
    let mut replacements: Vec<Replacement> = Vec::new();
    for op in changes.ops() {
        let (deleted, inserted) = match op {
            Operation::Retain(n) => {
                old += n;
                new += n;
                continue;
            }
            Operation::Delete(n) => (*n, ""),
            Operation::Insert(s) => (0, s.as_str()),
        };
        let breaks = inserted.contains(['\n', '\r']);
        match replacements.last_mut() {
            Some(last) if last.old.end == old && last.new.end == new => {
                last.old.end += deleted;
                last.new.end += inserted.len();
                last.breaks |= breaks;
            }
            _ => replacements.push(Replacement {
                old: old..old + deleted,
                new: new..new + inserted.len(),
                breaks,
            }),
        }
        old += deleted;
        new += inserted.len();
    }
    replacements
}

#[derive(Debug, Default)]
pub struct DisplayMap {
    config: LayoutConfig,
    /// Rows of each line while wrapping; empty otherwise.
    rows_per_line: Vec<u32>,
    /// `rows_before[i]` is the number of rows in lines `0..i`; valid up to `valid_prefix`.
    rows_before: Vec<usize>,
    valid_prefix: usize,
    /// Layouts of lines shown or looked at, by line.
    lines: HashMap<usize, LineLayout>,
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
        self.lines.clear();
        self.rows_per_line = rows;
        self.valid_prefix = 0;
    }

    /// Updates the layout after `changes` produced `text`.
    pub fn edit(&mut self, text: &Rope, changes: &ChangeSet) {
        self.carry_over(text, changes);
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
                let range = line_range(text, line);
                if range.len() > LONG_LINE {
                    self.row_starts(text, line).1.len() as u32
                } else {
                    row_count(text, range, width, self.config.tab_width) as u32
                }
            })
            .collect();
        self.rows_per_line.splice(first..old_end, rows);
        self.valid_prefix = self.valid_prefix.min(first);
        debug_assert_eq!(self.rows_per_line.len(), new_lines);
    }

    /// Keeps the layouts of long lines through an edit: lines the edit moved are moved, lines
    /// it changed are updated around the change. Other layouts are dropped and made again when
    /// needed.
    fn carry_over(&mut self, text: &Rope, changes: &ChangeSet) {
        let replacements = replacements(changes);
        let config = self.config;
        for (_, mut layout) in std::mem::take(&mut self.lines) {
            let old = layout.range.clone();
            if old.len() <= LONG_LINE {
                continue;
            }
            let first = replacements.partition_point(|r| r.old.end < old.start);
            let touching = &replacements[first..];
            let touching = &touching[..touching.partition_point(|r| r.old.start <= old.end)];
            // Replacements before the line move it.
            let moved = first.checked_sub(1).map_or(0, |last| {
                let last = &replacements[last];
                last.new.end as isize - last.old.end as isize
            });
            let start = old.start.strict_add_signed(moved);
            let grown: isize = touching
                .iter()
                .map(|r| r.new.len() as isize - r.old.len() as isize)
                .sum();
            let range = start..start + old.len().strict_add_signed(grown);
            let line = line_of(text, start);
            let within = touching
                .iter()
                .all(|r| !r.breaks && old.start <= r.old.start && r.old.end <= old.end);
            if !within || line_range(text, line) != range {
                continue;
            }
            if let (Some(first), Some(last)) = (touching.first(), touching.last()) {
                // Everything from the first to the last change, as one.
                let changed = first.old.start - old.start..last.old.end - old.start;
                let new_end = last.new.end - start;
                if let Some(columns) = &mut layout.columns {
                    columns.edit(
                        text,
                        range.clone(),
                        changed.clone(),
                        new_end,
                        config.tab_width,
                    );
                }
                if let (Some(rows), Some(width)) = (&layout.rows, config.wrap_width) {
                    let rows = rewrap(
                        text,
                        range.clone(),
                        rows,
                        changed,
                        new_end,
                        width,
                        config.tab_width,
                    );
                    layout.rows = Some(Arc::new(rows));
                }
            }
            layout.range = range;
            self.lines.insert(line, layout);
        }
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

    /// The kept layout of `line`.
    fn line_layout(&mut self, text: &Rope, line: usize) -> &mut LineLayout {
        let range = line_range(text, line);
        let layout = self
            .lines
            .entry(line)
            .or_insert_with(|| LineLayout::new(range.clone()));
        if layout.range != range {
            *layout = LineLayout::new(range);
        }
        layout
    }

    /// The start of a line and where its rows start (offsets from it, and columns), wrapped
    /// or not.
    fn row_starts(&mut self, text: &Rope, line: usize) -> (usize, Arc<Vec<(usize, usize)>>) {
        let LayoutConfig {
            tab_width,
            wrap_width,
        } = self.config;
        let Some(width) = wrap_width else {
            return (line_range(text, line).start, Arc::new(vec![(0, 0)]));
        };
        let layout = self.line_layout(text, line);
        let range = layout.range.clone();
        let rows = layout
            .rows
            .get_or_insert_with(|| Arc::new(line_rows(text, range.clone(), width, tab_width)));
        (range.start, rows.clone())
    }

    fn make_row(&mut self, text: &Rope, line: usize, index: usize) -> Row {
        let (line_start, starts) = self.row_starts(text, line);
        let index = index.min(starts.len() - 1);
        let (offset, start_column) = starts[index];
        let end = starts.get(index + 1).map_or_else(
            || line_range(text, line).end,
            |&(next, _)| line_start + next,
        );
        Row {
            line,
            index_in_line: index,
            range: line_start + offset..end,
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
        let (line_start, starts) = self.row_starts(text, line);
        let index = starts
            .partition_point(|&(offset, _)| line_start + offset <= pos)
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
            let (offset, column) = self
                .column_index(text, line)
                .before_offset(pos - range.start);
            (range.start + offset, column)
        } else {
            (range.start, 0)
        };
        column_after(text, from, column, pos, self.config.tab_width)
    }

    /// The position in `row` closest to `column` (counted from the start of the line).
    pub fn pos_at_column(&mut self, text: &Rope, row: &Row, column: usize) -> usize {
        let (from, from_column) = if row.range.len() > LONG_LINE {
            let line_start = line_range(text, row.line).start;
            let (offset, at) = self.column_index(text, row.line).before_column(column);
            let pos = line_start + offset;
            if (row.range.start..=row.range.end).contains(&pos) {
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
            let (offset, at) = self.column_index(text, line).before_column(column);
            (range.start + offset, at)
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

    fn column_index(&mut self, text: &Rope, line: usize) -> &ColumnIndex {
        let tab_width = self.config.tab_width;
        let layout = self.line_layout(text, line);
        let range = layout.range.clone();
        layout
            .columns
            .get_or_insert_with(|| ColumnIndex::build(text, range, tab_width))
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
    use proptest::prelude::*;

    fn wrapped(width: usize) -> LayoutConfig {
        LayoutConfig {
            tab_width: 4,
            wrap_width: Some(width),
        }
    }

    /// Lays out every line, as frames showing all of them would.
    fn show_all(map: &mut DisplayMap, text: &Rope) {
        for line in 0..line_count(text) {
            let end = line_range(text, line).end;
            map.column(text, end);
            map.row_starts(text, line);
        }
    }

    /// Checks that the layouts kept for lines are what laying them out afresh gives.
    fn check_kept(map: &DisplayMap, text: &Rope) -> Result<(), TestCaseError> {
        let LayoutConfig {
            tab_width,
            wrap_width,
        } = map.config;
        for (&line, layout) in &map.lines {
            let range = line_range(text, line);
            prop_assert_eq!(&layout.range, &range, "line {}", line);
            if let (Some(rows), Some(width)) = (&layout.rows, wrap_width) {
                prop_assert_eq!(
                    &**rows,
                    &line_rows(text, range.clone(), width, tab_width),
                    "rows of line {}",
                    line
                );
            }
            let Some(columns) = &layout.columns else {
                continue;
            };
            prop_assert_eq!(columns.points[0].offset, 0);
            for (index, point) in columns.points.iter().enumerate() {
                let pos = range.start + point.offset;
                let next = columns
                    .points
                    .get(index + 1)
                    .map_or(range.end, |next| range.start + next.offset);
                prop_assert!(pos < next || (pos == next && next == range.end));
                prop_assert!(text.is_char_boundary(pos));
                prop_assert_eq!(
                    point.column,
                    column_after(text, range.start, 0, pos, tab_width),
                    "column at {} of line {}",
                    point.offset,
                    line
                );
                prop_assert_eq!(
                    point.tab,
                    text.slice(pos..next).chars().any(|ch| ch == '\t')
                );
            }
        }
        Ok(())
    }

    fn some_text() -> impl Strategy<Value = String> {
        prop::collection::vec(
            prop::sample::select(vec![
                "a", "bb", " ", "\t", "\t", "日", "😀", "word ", "\n", "\r\n", "\u{301}",
            ]),
            0..80,
        )
        .prop_map(|parts| parts.concat())
    }

    /// Edits at character boundaries of `doc`, sorted and not overlapping.
    fn edits(doc: &str, raw: Vec<(usize, usize, String)>) -> Vec<Edit> {
        let boundaries: Vec<usize> = doc
            .char_indices()
            .map(|(i, _)| i)
            .chain([doc.len()])
            .collect();
        let mut edits: Vec<Edit> = raw
            .into_iter()
            .map(|(a, b, text)| {
                let a = boundaries[a % boundaries.len()];
                let b = boundaries[b % boundaries.len()];
                Edit::replace(a.min(b)..a.max(b).min(a.min(b) + 6), text)
            })
            .filter(|edit| doc.is_char_boundary(edit.range.end))
            .collect();
        edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
        let mut end = 0;
        edits.retain(|edit| {
            let keep = edit.range.start >= end;
            if keep {
                end = edit.range.end;
            }
            keep
        });
        edits
    }

    proptest! {
        #[test]
        fn long_lines_keep_their_layout_through_edits(
            doc in some_text(),
            steps in prop::collection::vec(
                prop::collection::vec(
                    (any::<usize>(), any::<usize>(), prop::sample::select(vec![
                        "", "x", "\t", "日", "ab ", "\t\t", "\n", "\u{301}",
                    ]).prop_map(str::to_owned)),
                    0..4,
                ),
                1..8,
            ),
            width in prop::option::of(1usize..12),
            tab_width in 1usize..5,
        ) {
            let config = LayoutConfig { tab_width, wrap_width: width };
            let mut text = Rope::from_str(&doc);
            let mut map = DisplayMap::new(&text, config);
            show_all(&mut map, &text);
            for raw in steps {
                let current = text.to_string();
                let changes = ChangeSet::from_edits(&text, edits(&current, raw)).unwrap();
                changes.apply(&mut text);
                map.edit(&text, &changes);
                check_kept(&map, &text)?;
                let fresh = DisplayMap::new(&text, config);
                prop_assert_eq!(map.rows_per_line(), fresh.rows_per_line());
                show_all(&mut map, &text);
            }
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
