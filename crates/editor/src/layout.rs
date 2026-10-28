//! Turning the cell layout of `birchpad-view` into pixels for one frame: which rows are visible,
//! their shaped text, selections, carets, line numbers and scrollbars.
//!
//! Only visible rows are shaped, and of a long row only the columns in view: a 10 MB line costs
//! the same per frame as a short one (once its column index exists).

mod compute;
mod decorations;
mod hit_test;
mod scrollbars;
mod shaping;

use std::ops::Range as ByteRange;

use birchpad_core::LineChange;
use birchpad_view::{DisplayText, Row};
use gpui_kit::{Bounds, Font, Pixels, Point, ShapedLine, point, px, size};

use super::ViewSettings;
use super::symbols::Symbols;
use super::theme::{Paint, TextStyle};

/// Documents larger than this are rewrapped on a background thread.
const BACKGROUND_WRAP: usize = 4 * 1024 * 1024;
/// Rows longer than this many bytes are shaped only around the visible columns.
const LONG_ROW: usize = 2048;
/// Extra columns shaped on each side of the visible ones in a long row.
const LONG_ROW_MARGIN: usize = 16;
pub const SCROLLBAR: Pixels = px(12.);
const TEXT_PADDING: Pixels = px(4.);
/// Columns kept between the caret and the left or right edge when scrolling horizontally.
const CARET_MARGIN_COLUMNS: f32 = 4.;

#[derive(Debug, Clone)]
pub struct Metrics {
    pub font: Font,
    pub font_size: Pixels,
    pub line_height: Pixels,
    /// Width of one cell of the monospace grid.
    pub cell: Pixels,
}

/// Gaps smaller than this between the parts of the text shown in a frame are highlighted with
/// one query; larger ones (inside a long, partly shown line) split the query.
const QUERY_GAP: usize = 4096;

/// Syntax highlights of the text shown in the last frame, reused while the tree and the
/// visible text stay the same (as when the caret blinks).
pub struct HighlightCache {
    language: &'static str,
    version: u64,
    /// The theme's generation: a new theme colors the text again.
    theme: u64,
    ranges: Vec<ByteRange<usize>>,
    spans: Vec<(ByteRange<usize>, TextStyle)>,
}

/// A row painted in this frame.
pub struct VisibleRow {
    pub row: Row,
    /// Top of the row, in window coordinates.
    pub y: Pixels,
    /// Where `display` starts, in window coordinates.
    pub x: Pixels,
    pub display: DisplayText,
    pub shaped: ShapedLine,
    /// The order the characters show in, when it is not the order they are stored in.
    pub bidi: Option<birchpad_view::BidiRow>,
}

impl VisibleRow {
    /// Whether the shaped text covers the whole row (short rows) or only the visible part.
    fn covers(&self, pos: usize) -> bool {
        self.display.doc_start() <= pos && pos <= self.display.doc_end()
    }

    pub fn x_for(&self, pos: usize) -> Option<Pixels> {
        self.covers(pos).then(|| {
            let index = self.display.to_display(pos);
            let index = self
                .bidi
                .as_ref()
                .map_or(index, |bidi| bidi.visual_caret(index));
            self.x + self.shaped.x_for_index(index)
        })
    }

    fn contains_caret(&self, pos: usize) -> bool {
        self.row.range.start <= pos
            && (pos < self.row.range.end || (pos == self.row.range.end && self.row.last_in_line))
    }
}

pub struct Scrollbar {
    pub track: Bounds<Pixels>,
    pub thumb: Bounds<Pixels>,
    /// Marks along the track: where Compare found differences (its navigation bar).
    pub markers: Vec<(Bounds<Pixels>, birchpad_theme::Color)>,
}

/// The margins left of the text, as in Notepad++: line numbers, symbols (bookmarks), the change
/// history, folding. A margin that is turned off is `None`.
pub struct Margins {
    /// All margins together.
    pub gutter: Bounds<Pixels>,
    pub line_numbers: Option<Bounds<Pixels>>,
    pub symbols: Option<Bounds<Pixels>>,
    pub changes: Option<Bounds<Pixels>>,
    pub folding: Option<Bounds<Pixels>>,
}

impl Margins {
    /// The margins at the left of `bounds`, or in right-to-left mode (`rtl`) mirrored at the
    /// right of the text, which ends at `right`: line numbers outermost, folding next to the
    /// text, as Scintilla mirrors them.
    fn new(
        bounds: Bounds<Pixels>,
        settings: &ViewSettings,
        metrics: &Metrics,
        line_count: usize,
        rtl: Option<Pixels>,
    ) -> Self {
        let mut x = rtl.unwrap_or(bounds.left());
        let mut column = |enabled: bool, width: Pixels| {
            enabled.then(|| {
                let left = if rtl.is_some() { x - width } else { x };
                let margin =
                    Bounds::new(point(left, bounds.top()), size(width, bounds.size.height));
                if rtl.is_some() {
                    x -= width;
                } else {
                    x += width;
                }
                margin
            })
        };
        // Wide enough for the largest line number (at least three digits), or for seven
        // digits whatever the document's length (Constant width).
        let digits = match settings.line_number_width {
            birchpad_config::LineNumberWidth::Dynamic => line_count.to_string().len().max(3),
            birchpad_config::LineNumberWidth::Constant => 7,
        };
        let line_numbers = column(settings.line_numbers, metrics.cell * (digits as f32 + 2.));
        let symbols = column(settings.bookmark_margin, metrics.line_height);
        let changes = column(
            settings.change_history,
            (metrics.line_height * 0.3).round().max(px(4.)),
        );
        let folding = column(settings.fold_margin, (metrics.line_height * 0.8).round());
        let gutter = match rtl {
            Some(right) => {
                Bounds::from_corners(point(x, bounds.top()), point(right, bounds.bottom()))
            }
            None => Bounds::from_corners(bounds.origin, point(x, bounds.bottom())),
        };
        Self {
            gutter,
            line_numbers,
            symbols,
            changes,
            folding,
        }
    }
}

/// A line sticky scroll keeps at the top of the view.
pub struct StickyRow {
    pub line: usize,
    /// Top of the row, in window coordinates.
    pub y: Pixels,
    /// Where its text starts.
    pub x: Pixels,
    pub shaped: ShapedLine,
    /// Its number in the line number margin.
    pub number: Option<(Point<Pixels>, ShapedLine)>,
}

pub struct Layout {
    pub metrics: Metrics,
    pub margins: Margins,
    pub text_bounds: Bounds<Pixels>,
    /// Window x of column 0 of an unwrapped row (scrolled).
    pub column_zero: Pixels,
    pub rows: Vec<VisibleRow>,
    pub line_numbers: Vec<(Point<Pixels>, ShapedLine)>,
    /// Bookmark symbols in the symbol margin.
    pub bookmarks: Vec<Bounds<Pixels>>,
    /// Bars of changed lines in the change history margin.
    pub change_bars: Vec<(Bounds<Pixels>, LineChange)>,
    /// Fold boxes in the folding margin, and whether each fold is collapsed.
    pub fold_boxes: Vec<(Bounds<Pixels>, bool)>,
    /// Lines with hidden lines below them (View > Hide Lines), with the band across the
    /// margins and the text that marks them; a click in the margins shows them again.
    pub hidden_marks: Vec<(usize, Bounds<Pixels>)>,
    /// The lines of expanded folds in the folding margin.
    pub fold_lines: Vec<Bounds<Pixels>>,
    /// A line under each collapsed fold's header, across the text.
    pub fold_underlines: Vec<Bounds<Pixels>>,
    /// Decoration ranges (token styles, matches) drawn behind or around the text.
    pub decorations: Vec<(Bounds<Pixels>, Paint)>,
    pub selections: Vec<Bounds<Pixels>>,
    pub carets: Vec<Bounds<Pixels>>,
    pub vertical_bar: Option<Scrollbar>,
    pub horizontal_bar: Option<Scrollbar>,
    /// Show Symbol, the edge and the current line.
    pub symbols: Symbols,
    pub total_rows: usize,
    /// Rows that fit completely.
    pub full_rows: usize,
    /// Sticky scroll's lines, from the top.
    pub sticky: Vec<StickyRow>,
    /// The carets are blocks over a character, drawn translucent.
    pub caret_block: bool,
    /// The line between the margins and the text.
    pub border_width: Pixels,
    pub fold_style: birchpad_config::FoldStyle,
}
