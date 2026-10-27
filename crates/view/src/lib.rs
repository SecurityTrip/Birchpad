//! Layout of a document view, independent of any UI toolkit.
//!
//! The editor draws text on a monospace grid of cells: a character takes one or two cells (wide
//! CJK characters and emoji take two), tabs advance to the next tab stop, control characters
//! are drawn as one-cell pictures, non-printing characters shown by label take its cells. Rows, word wrap and horizontal positions are computed in
//! cells here; the UI only converts cells to pixels and shapes the text of visible rows.

mod bidi;
mod block;
mod cells;
mod display_map;
mod folding;
mod guides;
mod wrap;

pub use bidi::BidiRow;
pub use block::{Block, BlockPoint};
pub use cells::{
    DisplayText, Grid, NonPrinting, NonPrintingLabel, abbreviation, cells_at, char_cells,
    column_after, control_picture, non_printing, non_printing_label, pos_at_column, tab_advance,
};
pub use display_map::{DisplayMap, LayoutConfig, LongLines, Row};
pub use folding::{
    Fold, fold_at, folds_from_ranges, hidden_lines, innermost_containing, shift_folds,
    sticky_headers,
};
pub use guides::{indent_guides, indentation};
pub use wrap::{WrapIndent, row_count, wrap_indent, wrap_line};
