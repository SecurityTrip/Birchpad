use birchpad_panels::function_list::*;
use birchpad_syntax::Symbol;
use birchpad_syntax::SymbolKind;
use std::collections::HashSet;
use std::ops::Range;

fn symbol(depth: usize, name: &str, range: Range<usize>) -> Symbol {
    Symbol {
        kind: SymbolKind::Function,
        name: name.into(),
        name_range: range.start..range.start + name.len(),
        range,
        depth,
    }
}

/// zeta { beta, alpha { inner } }, gamma
fn outline() -> Vec<Symbol> {
    vec![
        symbol(0, "zeta", 0..50),
        symbol(1, "beta", 5..10),
        symbol(1, "alpha", 10..40),
        symbol(2, "inner", 20..30),
        symbol(0, "gamma", 60..70),
    ]
}

fn names(symbols: &[Symbol], rows: &[Row]) -> Vec<String> {
    rows.iter()
        .map(|row| {
            let fold = match (row.has_children, row.collapsed) {
                (true, true) => "+",
                (true, false) => "-",
                _ => " ",
            };
            format!(
                "{}{fold}{}",
                "  ".repeat(row.depth),
                symbols[row.symbol].name
            )
        })
        .collect()
}

#[test]
fn the_tree_follows_the_text_or_the_names() {
    let symbols = outline();
    let none = HashSet::new();
    assert_eq!(
        names(&symbols, &rows(&symbols, false, &none, "")),
        ["-zeta", "   beta", "  -alpha", "     inner", " gamma"]
    );
    assert_eq!(
        names(&symbols, &rows(&symbols, true, &none, "")),
        [" gamma", "-zeta", "  -alpha", "     inner", "   beta"],
        "each level by name"
    );
    assert!(rows(&[], true, &none, "").is_empty());
}

#[test]
fn folded_symbols_hide_their_children() {
    let symbols = outline();
    let collapsed: HashSet<String> = ["zeta\u{1f}alpha".to_owned()].into();
    assert_eq!(
        names(&symbols, &rows(&symbols, false, &collapsed, "")),
        ["-zeta", "   beta", "  +alpha", " gamma"]
    );
    // A symbol without children is never shown folded, and an unknown path folds nothing.
    let leaf: HashSet<String> = ["gamma".to_owned(), "nowhere".to_owned()].into();
    assert_eq!(rows(&symbols, false, &leaf, "").len(), 5);
}

#[test]
fn a_filter_keeps_the_matches_and_their_parents() {
    let symbols = outline();
    let collapsed: HashSet<String> = ["zeta".to_owned()].into();
    assert_eq!(
        names(&symbols, &rows(&symbols, false, &collapsed, " INN ")),
        ["-zeta", "  -alpha", "     inner"],
        "without regard to case, inside a folded symbol too"
    );
    assert_eq!(
        names(&symbols, &rows(&symbols, false, &collapsed, "a")),
        ["-zeta", "   beta", "   alpha", " gamma"],
        "inner is filtered out, so alpha has nothing to fold"
    );
    assert!(rows(&symbols, false, &collapsed, "nothing").is_empty());
}

#[test]
fn the_current_symbol_is_the_innermost_around_the_caret() {
    let symbols = outline();
    assert_eq!(current_symbol(&symbols, 0), Some(0), "the first byte");
    assert_eq!(current_symbol(&symbols, 25), Some(3));
    assert_eq!(current_symbol(&symbols, 12), Some(2));
    assert_eq!(current_symbol(&symbols, 40), Some(0), "just past alpha");
    assert_eq!(current_symbol(&symbols, 50), None, "just past zeta");
    assert_eq!(current_symbol(&symbols, 55), None);
    assert_eq!(current_symbol(&[], 0), None);
}

// --- In the window -------------------------------------------------------------------------
