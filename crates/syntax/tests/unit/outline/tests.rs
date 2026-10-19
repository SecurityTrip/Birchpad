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
