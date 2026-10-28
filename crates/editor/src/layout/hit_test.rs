//! Finding what is under a point of the text area, and where a range is drawn.

use std::ops::Range as ByteRange;

use birchpad_core::Rope;
use birchpad_view::{BlockPoint, DisplayMap};
use gpui_kit::{Bounds, Pixels, Point, point};

use super::{Layout, TEXT_PADDING, VisibleRow};

impl Layout {
    /// Columns that fit across the text area.
    pub fn text_columns(&self) -> usize {
        let usable = self.text_bounds.size.width - TEXT_PADDING * 2.;
        ((usable / self.metrics.cell).floor() as usize).max(1)
    }

    /// The line of the sticky row at `y`, if one is there.
    pub fn sticky_at(&self, y: Pixels) -> Option<usize> {
        let height = self.metrics.line_height;
        self.sticky
            .iter()
            .find(|row| row.y <= y && y < row.y + height)
            .map(|row| row.line)
    }

    /// The visible row at window y, if any.
    pub fn row_at_y(&self, y: Pixels) -> Option<&VisibleRow> {
        self.rows
            .iter()
            .find(|row| row.y <= y && y < row.y + self.metrics.line_height)
    }

    /// The document position under a window point; above or below the rows, the nearest row.
    pub fn position_for_point(
        &self,
        point: Point<Pixels>,
        text: &Rope,
        display: &mut DisplayMap,
    ) -> Option<usize> {
        let row = self
            .rows
            .iter()
            .find(|row| point.y < row.y + self.metrics.line_height)
            .or_else(|| self.rows.last())?;
        let pos = if row.covers(row.row.range.end) && row.display.doc_start() == row.row.range.start
        {
            let index = row.shaped.closest_index_for_x(point.x - row.x);
            let index = row
                .bidi
                .as_ref()
                .map_or(index, |bidi| bidi.logical_caret(index));
            row.display.to_doc(index)
        } else {
            // A long row shaped only in part: go by columns.
            let column = ((point.x - self.column_zero) / self.metrics.cell)
                .round()
                .max(0.);
            display.pos_at_column(text, &row.row, row.row.start_column + column as usize)
        };
        // The end of a wrapped row is the start of the next one: stay on this row.
        if !row.row.last_in_line && pos >= row.row.range.end && pos > row.row.range.start {
            Some(birchpad_core::motion::prev_boundary(
                text,
                row.row.range.end,
            ))
        } else {
            Some(pos.clamp(row.row.range.start, row.row.range.end))
        }
    }

    /// The rectangle corner under a window point: the line and column of the nearest
    /// character boundary, or past the end of a line, the column under the point.
    pub fn block_point_for_point(
        &self,
        point: Point<Pixels>,
        text: &Rope,
        display: &mut DisplayMap,
    ) -> Option<BlockPoint> {
        let row = self
            .rows
            .iter()
            .find(|row| point.y < row.y + self.metrics.line_height)
            .or_else(|| self.rows.last())?;
        let pos = self.position_for_point(point, text, display)?;
        let column = display.column(text, pos);
        let mut virtual_cells = 0;
        if row.row.last_in_line && pos == row.row.range.end {
            let end_x = row.x_for(pos).unwrap_or_else(|| {
                self.column_zero + self.metrics.cell * (column - row.row.start_column) as f32
            });
            virtual_cells = ((point.x - end_x) / self.metrics.cell).round().max(0.) as usize;
        }
        Some(BlockPoint::new(row.row.line, column + virtual_cells))
    }

    /// Screen bounds of a range (for input method candidate windows).
    pub fn bounds_for_range(&self, range: ByteRange<usize>) -> Option<Bounds<Pixels>> {
        let row = self
            .rows
            .iter()
            .find(|row| row.contains_caret(range.start))?;
        let start = row.x_for(range.start)?;
        let end = row.x_for(range.end.min(row.row.range.end)).unwrap_or(start);
        Some(Bounds::from_corners(
            point(start, row.y),
            point(end.max(start), row.y + self.metrics.line_height),
        ))
    }
}
