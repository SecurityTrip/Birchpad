//! The crate's tests through its public interface, one module per part of `src`, in one
//! test binary.

#[path = "change/tests.rs"]
mod change;

#[path = "change_history/next_change_tests.rs"]
mod change_history_next_change_tests;

#[path = "change_history/tests.rs"]
mod change_history;

#[path = "completion/tests.rs"]
mod completion;

#[path = "decoration/size_tests.rs"]
mod decoration_size_tests;

#[path = "decoration/tests.rs"]
mod decoration;

#[path = "document/tests.rs"]
mod document;

#[path = "format/tests.rs"]
mod format;

#[path = "hash/tests.rs"]
mod hash;

#[path = "line_ending/tests.rs"]
mod line_ending;

#[path = "links/tests.rs"]
mod links;

#[path = "motion/count_tests.rs"]
mod motion_count_tests;

#[path = "motion/go_to_tests.rs"]
mod motion_go_to_tests;

#[path = "motion/tests.rs"]
mod motion;

#[path = "ops/blank/tests.rs"]
mod ops_blank;

#[path = "ops/case/tests.rs"]
mod ops_case;

#[path = "ops/column/properties.rs"]
mod ops_column_properties;

#[path = "ops/column/tests.rs"]
mod ops_column;

#[path = "ops/comment/tests.rs"]
mod ops_comment;

#[path = "ops/encode/tests.rs"]
mod ops_encode;

#[path = "ops/indent/tests.rs"]
mod ops_indent;

#[path = "ops/lines/tests.rs"]
mod ops_lines;

#[path = "ops/marks/tests.rs"]
mod ops_marks;

#[path = "ops/multi/tests.rs"]
mod ops_multi;

#[path = "ops/pairs/tests.rs"]
mod ops_pairs;

#[path = "ops/sort/tests.rs"]
mod ops_sort;

#[path = "ops/tags/tests.rs"]
mod ops_tags;

#[path = "ops/test_support.rs"]
mod ops_test_support;

#[path = "search/line_hit_tests.rs"]
mod search_line_hit_tests;

#[path = "search/range_tests.rs"]
mod search_range_tests;

#[path = "search/replace_properties.rs"]
mod search_replace_properties;

#[path = "search/tests.rs"]
mod search;

#[path = "selection/tests.rs"]
mod selection;

#[path = "timing/tests.rs"]
mod timing;

#[path = "transaction/tests.rs"]
mod transaction;
