//! Languages and syntax for Birchpad: recognizing a file's language, keeping a tree-sitter
//! syntax tree in step with the text, and answering what the editor asks of it (highlights of
//! the visible text, matching brackets). No UI dependencies.

mod brackets;
mod folds;
mod highlight;
mod language;
mod syntax;

pub use brackets::{BracketMatch, matching_bracket};
pub use folds::{fold_ranges, indent_fold_ranges};
pub use highlight::{HIGHLIGHT_NAMES, Highlight, resolve};
pub use language::{LANGUAGES, Language, by_id, detect};
pub use syntax::{LanguageConfig, ParseJob, Syntax, config};
pub use tree_sitter::Tree;
