//! The outline of a document, for the Function List: its classes, functions, methods and
//! sections, nested as they are in the text.
//!
//! Definitions are found with tree-sitter tags queries, the ones GitHub's code navigation uses:
//! `@definition.function` and its kin capture a definition, `@name` its name. Most grammar
//! crates ship one; Birchpad adds its own where they do not (`queries/tags`). References and
//! constants of those queries are left out: the Function List lists what defines code. A
//! definition inside another one's range is its child, so methods list under their class and
//! Markdown sections under their parent heading.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::ops::{ControlFlow, Range};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use birchpad_core::Rope;
use tree_sitter::{Query, QueryCursor, QueryCursorOptions, StreamingIterator};

use crate::language::Language;
use crate::syntax::{RopeText, Syntax, lua_matches_to_regex};

/// Longest an outline may take; a huge file gets the definitions found by then.
const BUDGET: Duration = Duration::from_millis(200);

/// What a definition is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    /// Classes, structs, enums, objects, `impl` blocks.
    Class,
    /// Interfaces, traits, type classes.
    Interface,
    /// Modules, namespaces, packages.
    Module,
    Function,
    Method,
    Macro,
    /// Type aliases.
    Type,
    /// Headings, INI sections, TOML tables.
    Section,
}

impl SymbolKind {
    /// The kind a tags query capture names, if the outline shows it.
    fn of_capture(name: &str) -> Option<Self> {
        let kind = name.strip_prefix("definition.")?;
        Some(match kind {
            "class" | "struct" | "enum" | "union" | "object" => Self::Class,
            "interface" | "trait" => Self::Interface,
            "module" | "namespace" | "package" => Self::Module,
            "function" => Self::Function,
            "method" | "constructor" => Self::Method,
            "macro" => Self::Macro,
            "type" => Self::Type,
            "section" | "heading" => Self::Section,
            _ => return None,
        })
    }
}

/// A definition of the outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub kind: SymbolKind,
    /// The name, on one line.
    pub name: String,
    /// The whole definition.
    pub range: Range<usize>,
    /// Its name in the text.
    pub name_range: Range<usize>,
    /// How many definitions it is inside of.
    pub depth: usize,
}

/// The tags query sources of `language`; empty if it has none.
fn sources(language: &Language) -> &'static [&'static str] {
    match language.id {
        "bash" => &[include_str!("../queries/tags/bash.scm")],
        "batch" => &[include_str!("../queries/tags/batch.scm")],
        "c" => &[tree_sitter_c::TAGS_QUERY],
        "cs" => &[tree_sitter_c_sharp::TAGS_QUERY],
        "cpp" => &[tree_sitter_cpp::TAGS_QUERY],
        "dart" => &[tree_sitter_dart::TAGS_QUERY],
        "elixir" => &[tree_sitter_elixir::TAGS_QUERY],
        "go" => &[tree_sitter_go::TAGS_QUERY],
        "haskell" => &[include_str!("../queries/tags/haskell.scm")],
        "ini" => &[include_str!("../queries/tags/ini.scm")],
        "java" => &[tree_sitter_java::TAGS_QUERY],
        "javascript" => &[tree_sitter_javascript::TAGS_QUERY],
        "kotlin" => &[include_str!("../queries/tags/kotlin.scm")],
        "lua" => &[tree_sitter_lua::TAGS_QUERY],
        "makefile" => &[include_str!("../queries/tags/makefile.scm")],
        "markdown" => &[include_str!("../queries/tags/markdown.scm")],
        "php" => &[tree_sitter_php::TAGS_QUERY],
        "powershell" => &[include_str!("../queries/tags/powershell.scm")],
        "python" => &[tree_sitter_python::TAGS_QUERY],
        "r" => &[tree_sitter_r::TAGS_QUERY],
        "ruby" => &[tree_sitter_ruby::TAGS_QUERY],
        "rust" => &[
            tree_sitter_rust::TAGS_QUERY,
            include_str!("../queries/tags/rust.scm"),
        ],
        "scala" => &[include_str!("../queries/tags/scala.scm")],
        "solidity" => &[tree_sitter_solidity::TAGS_QUERY],
        "swift" => &[tree_sitter_swift::TAGS_QUERY],
        "toml" => &[include_str!("../queries/tags/toml.scm")],
        "typescript" | "tsx" => &[tree_sitter_typescript::TAGS_QUERY],
        _ => &[],
    }
}

/// Whether the Function List has anything to show for `language`.
pub fn has_outline(language: &Language) -> bool {
    !sources(language).is_empty()
}

/// A compiled tags query.
struct Tags {
    query: Query,
    /// The index of `@name`.
    name: u32,
    /// The kind of each capture, if it is a definition the outline shows.
    kinds: Vec<Option<SymbolKind>>,
}

/// Compiled tags queries by language id; `None` for a language without one.
type Compiled = Mutex<HashMap<&'static str, Option<Arc<Tags>>>>;

/// The compiled tags query of `language`, compiled on first use.
fn tags(language: &'static Language) -> Option<Arc<Tags>> {
    static TAGS: OnceLock<Compiled> = OnceLock::new();
    let mut compiled = TAGS.get_or_init(Mutex::default).lock().ok()?;
    compiled
        .entry(language.id)
        .or_insert_with(|| compile(language).ok().map(Arc::new))
        .clone()
}

fn compile(language: &'static Language) -> Result<Tags, String> {
    let sources = sources(language);
    if sources.is_empty() {
        return Err(format!("{} has no tags query", language.name));
    }
    let grammar = (language.grammar)();
    // Directives such as `#strip!` and `#select-adjacent!` shape documentation comments, which
    // the outline does not show: their patterns stay, the directives are ignored.
    let query = Query::new(&grammar, &lua_matches_to_regex(&sources.join("\n")))
        .map_err(|error| format!("tags query of {}: {error}", language.name))?;
    let name = query
        .capture_index_for_name("name")
        .ok_or_else(|| format!("tags query of {} has no @name", language.name))?;
    let kinds = query
        .capture_names()
        .iter()
        .map(|capture| SymbolKind::of_capture(capture))
        .collect();
    Ok(Tags { query, name, kinds })
}

/// The definitions of the document, in text order, each with its depth among the others.
/// Embedded languages (a script in HTML) are not outlined.
pub fn outline(syntax: &Syntax, text: &Rope) -> Vec<Symbol> {
    let (Some(tree), Some(tags)) = (syntax.tree(), tags(syntax.language())) else {
        return Vec::new();
    };
    let started = Instant::now();
    let mut over_budget = |_: &tree_sitter::QueryCursorState| {
        if started.elapsed() > BUDGET {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    };
    let options = QueryCursorOptions::new().progress_callback(&mut over_budget);
    let mut cursor = QueryCursor::new();
    let mut matches =
        cursor.matches_with_options(&tags.query, tree.root_node(), RopeText(text), options);
    let mut symbols = Vec::new();
    while let Some(matched) = matches.next() {
        let mut name = None;
        let mut definition = None;
        for capture in matched.captures() {
            if capture.index == tags.name {
                name = Some(capture.node.byte_range());
            }
            if let Some(kind) = tags.kinds[capture.index as usize] {
                definition = Some((kind, capture.node.byte_range()));
            }
        }
        let (Some(name_range), Some((kind, range))) = (name, definition) else {
            continue;
        };
        let name = text
            .slice(name_range.clone())
            .to_string()
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_owned();
        if name.is_empty() {
            continue;
        }
        symbols.push(Symbol {
            kind,
            name,
            range,
            name_range,
            depth: 0,
        });
    }
    nest(symbols)
}

/// Sorts `symbols` by position, keeps one of each definition, and sets their depths. A
/// definition two patterns matched keeps the more telling kind: a function inside an `impl`
/// is a method.
fn nest(mut symbols: Vec<Symbol>) -> Vec<Symbol> {
    symbols.sort_by_key(|symbol| (symbol.range.start, Reverse(symbol.range.end)));
    symbols.dedup_by(|later, earlier| {
        let same = later.range == earlier.range && later.name_range == earlier.name_range;
        if same && earlier.kind == SymbolKind::Function {
            earlier.kind = later.kind;
        }
        same
    });
    let mut open: Vec<usize> = Vec::new();
    for symbol in &mut symbols {
        while open.last().is_some_and(|&end| end <= symbol.range.start) {
            open.pop();
        }
        symbol.depth = open.len();
        open.push(symbol.range.end);
    }
    symbols
}

#[cfg(test)]
#[path = "../tests/unit/outline/tests.rs"]
mod tests;
