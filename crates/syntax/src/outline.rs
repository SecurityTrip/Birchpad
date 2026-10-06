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
mod tests {
    use super::*;
    use crate::{LANGUAGES, by_id, config};

    fn symbols(language: &str, source: &str) -> Vec<(usize, SymbolKind, String)> {
        let text = Rope::from_str(source);
        let mut syntax = Syntax::new(config(by_id(language).unwrap()).unwrap());
        let parsed = syntax
            .parse_job(text.clone())
            .run(&std::sync::atomic::AtomicBool::new(false))
            .unwrap();
        syntax.install(parsed);
        outline(&syntax, &text)
            .into_iter()
            .map(|symbol| (symbol.depth, symbol.kind, symbol.name))
            .collect()
    }

    use SymbolKind::*;

    #[test]
    fn every_tags_query_compiles() {
        let mut outlined = 0;
        for language in LANGUAGES {
            if has_outline(language) {
                assert!(
                    tags(language).is_some(),
                    "{}: {:?}",
                    language.id,
                    compile(language).err()
                );
                outlined += 1;
            } else {
                assert!(tags(language).is_none(), "{}", language.id);
            }
        }
        assert!(outlined >= 28, "{outlined}");
    }

    #[test]
    fn rust_methods_list_under_their_impl() {
        let source = "struct Point { x: i32 }\n\
                      impl Point {\n    fn new() -> Self { todo!() }\n    fn x(&self) -> i32 { self.x }\n}\n\
                      fn main() {}\n\
                      macro_rules! twice { () => {} }\n\
                      mod inner { pub fn helper() {} }\n";
        assert_eq!(
            symbols("rust", source),
            [
                (0, Class, "Point".into()),
                (0, Class, "Point".into()),
                (1, Method, "new".into()),
                (1, Method, "x".into()),
                (0, Function, "main".into()),
                (0, Macro, "twice".into()),
                (0, Module, "inner".into()),
                // The grammar's query takes any function in a block of items for a method.
                (1, Method, "helper".into()),
            ]
        );
    }

    #[test]
    fn python_classes_hold_their_methods_and_constants_are_left_out() {
        let source = "LIMIT = 3\n\nclass Shape:\n    def area(self):\n        return 0\n\n    class Inner:\n        pass\n\ndef main():\n    pass\n";
        assert_eq!(
            symbols("python", source),
            [
                (0, Class, "Shape".into()),
                (1, Function, "area".into()),
                (1, Class, "Inner".into()),
                (0, Function, "main".into()),
            ]
        );
    }

    #[test]
    fn markdown_sections_nest_by_heading_level() {
        let source = "# Title\n\ntext\n\n## First\n\n### Deeper\n\n## Second\nSetext\n------\n";
        assert_eq!(
            symbols("markdown", source),
            [
                (0, Section, "Title".into()),
                (1, Section, "First".into()),
                (2, Section, "Deeper".into()),
                (1, Section, "Second".into()),
                (2, Section, "Setext".into()),
            ]
        );
    }

    #[test]
    fn own_queries_find_their_definitions() {
        assert_eq!(
            symbols("bash", "greet() { echo hi; }\nfunction bye { :; }\n"),
            [(0, Function, "greet".into()), (0, Function, "bye".into())]
        );
        assert_eq!(
            symbols("ini", "[one]\na=1\n[two]\n"),
            [(0, Section, "one".into()), (0, Section, "two".into())]
        );
        assert_eq!(
            symbols("toml", "[package]\nname = \"x\"\n[[bin]]\n[a.b]\n"),
            [
                (0, Section, "package".into()),
                (0, Section, "bin".into()),
                (0, Section, "a.b".into()),
            ]
        );
        assert_eq!(
            symbols("makefile", "all: build\n\tcc x.c\n\nclean:\n\trm x\n"),
            [(0, Function, "all".into()), (0, Function, "clean".into())]
        );
        assert_eq!(
            symbols("batch", "@echo off\ngoto end\n:start\necho x\n:end\n"),
            [(0, Function, ":start".into()), (0, Function, ":end".into())]
        );
    }

    #[test]
    fn languages_without_an_outline_and_empty_documents_list_nothing() {
        assert!(symbols("json", "{\"a\": 1}").is_empty());
        assert!(!has_outline(by_id("json").unwrap()));
        assert!(symbols("rust", "").is_empty());
        assert!(symbols("python", "x = 1\n").is_empty(), "only a constant");
        // A syntax that has not parsed yet has no tree.
        let syntax = Syntax::new(config(by_id("rust").unwrap()).unwrap());
        assert!(outline(&syntax, &Rope::from_str("fn main() {}")).is_empty());
    }

    #[test]
    fn broken_code_still_outlines_what_parses() {
        // An unclosed function: tree-sitter recovers, and the definitions around it stay.
        let found = symbols("rust", "fn first() {}\nfn broken( {\nfn last() {}\n");
        assert!(
            found.iter().any(|(_, _, name)| name == "first"),
            "{found:?}"
        );
    }

    #[test]
    fn nesting_follows_ranges_and_drops_repeats() {
        let symbol = |range: Range<usize>, name: &str| Symbol {
            kind: Function,
            name: name.into(),
            name_range: range.start..range.start + 1,
            range,
            depth: 9,
        };
        let nested = nest(vec![
            symbol(10..20, "sibling"),
            symbol(0..10, "outer"),
            symbol(2..5, "inner"),
            Symbol {
                kind: Method,
                ..symbol(2..5, "inner again")
            },
            symbol(5..10, "touching"),
            symbol(20..20, "empty"),
        ]);
        let shown: Vec<(usize, &str, SymbolKind)> = nested
            .iter()
            .map(|symbol| (symbol.depth, symbol.name.as_str(), symbol.kind))
            .collect();
        assert_eq!(
            shown,
            [
                (0, "outer", Function),
                (1, "inner", Method),
                (1, "touching", Function),
                (0, "sibling", Function),
                (0, "empty", Function),
            ],
            "one of the two matches of a definition, as a method"
        );
        assert!(nest(Vec::new()).is_empty());
    }

    #[test]
    fn captures_name_the_kinds_the_outline_shows() {
        assert_eq!(SymbolKind::of_capture("definition.struct"), Some(Class));
        assert_eq!(SymbolKind::of_capture("definition.trait"), Some(Interface));
        assert_eq!(
            SymbolKind::of_capture("definition.constructor"),
            Some(Method)
        );
        assert_eq!(SymbolKind::of_capture("definition.heading"), Some(Section));
        for left_out in [
            "definition.constant",
            "definition.variable",
            "reference.call",
            "name",
            "doc",
            "definition.",
        ] {
            assert_eq!(SymbolKind::of_capture(left_out), None, "{left_out}");
        }
    }
}
