//! Layout of a document view, independent of any UI toolkit.
//!
//! The editor draws text on a monospace grid of cells: a character takes one or two cells (wide
//! CJK characters and emoji take two), tabs advance to the next tab stop, control characters
//! are drawn as one-cell pictures. Rows, word wrap and horizontal positions are computed in
//! cells here; the UI only converts cells to pixels and shapes the text of visible rows.

mod block;
mod cells;
mod display_map;
mod folding;
mod guides;
mod wrap;

pub use block::{Block, BlockPoint};
pub use cells::{
    DisplayText, cells_at, char_cells, column_after, control_picture, pos_at_column, tab_advance,
};
pub use display_map::{DisplayMap, LayoutConfig, Row};
pub use folding::{
    Fold, fold_at, folds_from_ranges, hidden_lines, innermost_containing, shift_folds,
};
pub use guides::{indent_guides, indentation};
pub use wrap::{row_count, wrap_line};
