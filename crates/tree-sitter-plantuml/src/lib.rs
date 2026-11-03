//! PlantUML for tree-sitter, written for Birchpad: a grammar of lines rather than of diagrams.
//! A diagram runs from its `@start` line to its `@end` line; its blocks (`alt` ... `end`,
//! `box` ... `end box`, `!if` ... `!endif`, `!procedure` ... `!endprocedure`, a line ending in
//! `{` ... `}`, a note of several lines) are nodes of their own, so that they fold; any other
//! line is a row of tokens (keywords, arrows, strings, colors, stereotypes, labels).
//!
//! `src/parser.c` is generated from `grammar.js` by the tree-sitter CLI of the version the
//! workspace's `tree-sitter` has (`npx tree-sitter-cli@0.27.1 generate` in this folder); the
//! two are changed together.

use tree_sitter_language::LanguageFn;

// SAFETY: `tree_sitter_plantuml` is the function `src/parser.c` defines, compiled by
// `build.rs`; it takes nothing and returns a pointer to a static `TSLanguage`, as every
// tree-sitter parser does.
#[allow(unsafe_code)]
unsafe extern "C" {
    fn tree_sitter_plantuml() -> *const ();
}

/// The grammar, for `tree_sitter::Parser::set_language`.
// SAFETY: as above; `LanguageFn` asks for that function and nothing else.
#[allow(unsafe_code)]
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_plantuml) };

/// The kinds of nodes the grammar makes, as `tree-sitter generate` lists them.
pub const NODE_TYPES: &str = include_str!("node-types.json");
