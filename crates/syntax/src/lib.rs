//! Languages and syntax for Birchpad: recognizing a file's language, keeping a tree-sitter
//! syntax tree in step with the text (with trees of the languages embedded in it), and
//! answering what the editor asks of it (highlights of the visible text, matching brackets).
//! No UI dependencies.

mod brackets;
mod folds;
mod highlight;
mod language;
mod outline;
mod selection;
mod syntax;
mod udl;

pub use brackets::{BracketMatch, matching_bracket};
pub use folds::{fold_ranges, indent_fold_ranges};
pub use highlight::{HIGHLIGHT_NAMES, Highlight, resolve};
pub use language::{LANGUAGES, Language, all as all_languages, by_id, detect, injected};
pub use outline::{Symbol, SymbolKind, has_outline, outline, outline_query_error};
pub use selection::expand_selection;
pub use syntax::{
    LanguageConfig, Layer, MAX_DEPTH, ParseJob, Parsed, Syntax, config, lua_matches_to_regex,
    lua_pattern_to_regex,
};
pub use tree_sitter::{Node, Tree};
pub use udl::{
    DELIMITERS, LISTS, LineCommentAt, UdlDefinition, UdlError, UdlKind, UdlStyle, UserLanguage,
    nesting, parse_udl, udl_file,
};
