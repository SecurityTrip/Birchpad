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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LayoutConfig;

    fn ranges(text: &str, block: Block) -> Vec<Range> {
        let rope = Rope::from_str(text);
        let mut display = DisplayMap::new(&rope, LayoutConfig::default());
        block.selection(&rope, &mut display).ranges().to_vec()
    }

    #[test]
    fn a_rectangle_reaches_into_virtual_space() {
        let text = "abcdef\nab\n\nabcdef";
        let block = Block::new(BlockPoint::new(0, 1), BlockPoint::new(3, 4));
        assert_eq!(
            ranges(text, block),
            [
                Range::new(1, 4),
                Range::new(8, 9).with_virtual(0, 2),
                Range::new(10, 10).with_virtual(1, 4),
                Range::new(12, 15),
            ]
        );
    }

    #[test]
    fn a_rectangle_selected_leftwards_is_backward() {
        let block = Block::new(BlockPoint::new(1, 3), BlockPoint::new(0, 1));
        let rope = Rope::from_str("abcd\nx");
        let mut display = DisplayMap::new(&rope, LayoutConfig::default());
        let selection = block.selection(&rope, &mut display);
        assert_eq!(
            selection.ranges(),
            [Range::new(3, 1), Range::new(6, 6).with_virtual(2, 0)]
        );
        assert_eq!(selection.primary_index(), 0, "the caret's line is primary");
    }

    #[test]
    fn edges_follow_cells_across_tabs_and_wide_characters() {
        // Columns: "\t" 0-4, "x" 4; "日本" 0-2, 2-4, "y" 4.
        let text = "\tx\n日本y";
        let block = Block::new(BlockPoint::new(0, 4), BlockPoint::new(1, 5));
        assert_eq!(ranges(text, block), [Range::new(1, 2), Range::new(9, 10)]);
        // An edge inside a wide character rounds to its nearer side.
        let block = Block::new(BlockPoint::new(1, 1), BlockPoint::new(1, 3));
        assert_eq!(ranges(text, block), [Range::new(6, 9)]);
    }

    #[test]
    fn a_thin_rectangle_is_a_caret_on_each_line() {
        let thin = Block::new(BlockPoint::new(0, 2), BlockPoint::new(2, 2));
        assert!(thin.is_thin());
        assert!(!Block::new(BlockPoint::new(0, 2), BlockPoint::new(2, 3)).is_thin());
        assert_eq!(
            ranges("abcd\nab\nabcd", thin),
            [Range::point(2), Range::point(7), Range::point(10)]
        );
    }

    #[test]
    fn a_rectangle_past_the_last_line_stops_there() {
        let text = "ab\ncd";
        // From the last line down past it: only the lines that exist.
        let block = Block::new(BlockPoint::new(1, 0), BlockPoint::new(5, 1));
        assert_eq!(ranges(text, block), [Range::new(3, 4)]);
        // Entirely below the text: a caret at its end.
        let block = Block::new(BlockPoint::new(3, 0), BlockPoint::new(4, 1));
        assert_eq!(ranges(text, block), [Range::point(5)]);
        // In an empty text: the one empty line, with virtual space.
        let block = Block::new(BlockPoint::new(0, 0), BlockPoint::new(0, 2));
        assert_eq!(ranges("", block), [Range::new(0, 0).with_virtual(0, 2)]);
    }

    #[test]
    fn hidden_lines_are_skipped() {
        let rope = Rope::from_str("a\nb\nc\nd");
        let mut display = DisplayMap::new(&rope, LayoutConfig::default());
        display.set_hidden(std::iter::once(1..3).collect());
        let block = Block::new(BlockPoint::new(0, 0), BlockPoint::new(3, 1));
        let selection = block.selection(&rope, &mut display);
        assert_eq!(selection.ranges(), [Range::new(0, 1), Range::new(6, 7)]);
        assert_eq!(selection.primary(), Range::new(6, 7));
        assert_eq!(display.block_point(&rope, 7, 2), BlockPoint::new(3, 3));
    }
}
