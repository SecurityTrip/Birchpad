use super::*;
use crate::language::by_id;
use crate::syntax::config;
use std::sync::atomic::AtomicBool;

fn rust(source: &str) -> (Rope, Syntax) {
    let text = Rope::from_str(source);
    let mut syntax = Syntax::new(config(by_id("rust").unwrap()).unwrap());
    let tree = syntax
        .parse_job(text.clone())
        .run(&AtomicBool::new(false))
        .unwrap();
    syntax.install(tree);
    (text, syntax)
}

#[test]
fn plain_text_counts_brackets() {
    let text = Rope::from_str("a(b[c]d)e)");
    let at = |caret| matching_bracket(&text, None, caret);
    // Before the caret wins over after it.
    assert_eq!(at(2).unwrap().partner, Some(7..8));
    assert_eq!(at(1).unwrap().bracket, 1..2, "after the caret");
    assert_eq!(at(6).unwrap().partner, Some(3..4));
    assert_eq!(at(10).unwrap().partner, None, "unmatched");
    assert_eq!(at(0), None);
}

#[test]
fn syntax_ignores_brackets_in_strings_and_comments() {
    let source = "fn f() { g(\"(\"); } // (";
    let (text, syntax) = rust(source);
    let at = |caret| matching_bracket(&text, Some(&syntax), caret);
    // The `(` of `g(` matches the `)` after the string, not the one in it.
    let call = source.find("g(").unwrap() + 2;
    let close = source.find(");").unwrap();
    assert_eq!(at(call).unwrap().partner, Some(close..close + 1));
    assert_eq!(
        at(source.find('{').unwrap() + 1)
            .unwrap()
            .partner
            .map(|r| r.start),
        source.rfind('}')
    );
    // A bracket in a string or a comment does not match anything.
    assert_eq!(at(source.find("\"(").unwrap() + 2), None);
    assert_eq!(at(source.len()), None);
}
