//! Rectangular (column) selections: a block of lines and columns, turned into one selection
//! range per line.
//!
//! The rectangle is kept in lines and cells rather than byte positions, as Scintilla keeps it
//! in x coordinates: its edges stay straight across tabs and wide characters, and it reaches
//! into virtual space past the end of short lines.

use std::ops::RangeInclusive;

use birchpad_core::motion::line_of;
use birchpad_core::{Range, Rope, Selection};

use crate::display_map::DisplayMap;

/// A corner of a rectangle: a line and a column in cells from the start of the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockPoint {
    pub line: usize,
    pub column: usize,
}

impl BlockPoint {
    pub const fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

/// A rectangular selection from the corner where selecting started to the corner with the
/// caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Block {
    pub anchor: BlockPoint,
    pub head: BlockPoint,
}

impl Block {
    pub const fn new(anchor: BlockPoint, head: BlockPoint) -> Self {
        Self { anchor, head }
    }

    pub fn lines(&self) -> RangeInclusive<usize> {
        self.anchor.line.min(self.head.line)..=self.anchor.line.max(self.head.line)
    }

    pub fn left(&self) -> usize {
        self.anchor.column.min(self.head.column)
    }

    pub fn right(&self) -> usize {
        self.anchor.column.max(self.head.column)
    }

    /// A zero-width rectangle: a caret on each line.
    pub fn is_thin(&self) -> bool {
        self.anchor.column == self.head.column
    }

    /// One range per line of the rectangle, top to bottom, each from the left to the right
    /// edge (reversed when the caret is on the left), reaching into virtual space past the
    /// ends of short lines. Lines hidden by collapsed folds are left out. The range on the
    /// caret's line is primary.
    pub fn selection(&self, text: &Rope, display: &mut DisplayMap) -> Selection {
        let (left, right) = (self.left(), self.right());
        let backward = self.head.column < self.anchor.column;
        let mut ranges = Vec::new();
        let mut primary = 0;
        let last_line = birchpad_core::motion::line_count(text) - 1;
        for line in self.lines() {
            if line > last_line {
                break;
            }
            if display.is_hidden(line) && line != self.head.line {
                continue;
            }
            let (start, start_virtual) = display.pos_at_line_column(text, line, left);
            let (end, end_virtual) = display.pos_at_line_column(text, line, right);
            let range = Range::new(start, end).with_virtual(start_virtual, end_virtual);
            let range = if backward {
                Range::new(range.head, range.anchor).with_virtual(end_virtual, start_virtual)
            } else {
                range
            };
            if line <= self.head.line {
                primary = ranges.len();
            }
            ranges.push(range);
        }
        if ranges.is_empty() {
            return Selection::point(text.len());
        }
        Selection::new(ranges, primary)
    }
}

impl DisplayMap {
    /// The rectangle corner at `pos` plus `virtual_cells` past it.
    pub fn block_point(&mut self, text: &Rope, pos: usize, virtual_cells: usize) -> BlockPoint {
        BlockPoint::new(line_of(text, pos), self.column(text, pos) + virtual_cells)
    }
}
