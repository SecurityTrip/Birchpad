//! Languages and syntax for Birchpad: recognizing a file's language, keeping a tree-sitter
//! syntax tree in step with the text (with trees of the languages embedded in it), and
//! answering what the editor asks of it (highlights of the visible text, matching brackets).
//! No UI dependencies.

mod brackets;
mod folds;
mod highlight;
mod language;
mod outline;
mod syntax;

pub use brackets::{BracketMatch, matching_bracket};
pub use folds::{fold_ranges, indent_fold_ranges};
pub use highlight::{HIGHLIGHT_NAMES, Highlight, resolve};
pub use language::{LANGUAGES, Language, by_id, detect, injected};
pub use outline::{Symbol, SymbolKind, has_outline, outline};
pub use syntax::{LanguageConfig, Layer, ParseJob, Parsed, Syntax, config};
pub use tree_sitter::{Node, Tree};
