//! The crate's tests through its public interface, one module per part of `src`, in one
//! test binary.

#[path = "bidi_tests.rs"]
mod bidi_tests;

#[path = "buffer/tests.rs"]
mod buffer;

#[path = "capture_coverage_tests.rs"]
mod capture_coverage_tests;

#[path = "column_editor/tests.rs"]
mod column_editor;

#[path = "completion/tests.rs"]
mod completion;

#[path = "edge_tests.rs"]
mod edge_tests;

#[path = "editing_options/tests.rs"]
mod editing_options;

#[allow(
    clippy::single_range_in_vec_init,
    reason = "hidden lines are lists of line ranges, some with one range"
)]
#[path = "folding/tests.rs"]
mod folding;

#[allow(
    clippy::single_range_in_vec_init,
    reason = "decorations are lists of byte ranges, some with one range"
)]
#[path = "marks/tests.rs"]
mod marks;

#[path = "multi/gpui_tests.rs"]
mod multi_gpui_tests;

#[path = "multi/tests.rs"]
mod multi;

#[path = "operations/tests.rs"]
mod operations;

#[allow(
    clippy::single_range_in_vec_init,
    reason = "decorations are lists of byte ranges, some with one range"
)]
#[path = "smart_highlight/tests.rs"]
mod smart_highlight;

#[path = "symbols/tests.rs"]
mod symbols;

#[path = "syntax_selection/tests.rs"]
mod syntax_selection;

#[path = "test_support.rs"]
mod test_support;

#[path = "tests.rs"]
mod lib_tests;

#[path = "theme/tests.rs"]
mod theme;
