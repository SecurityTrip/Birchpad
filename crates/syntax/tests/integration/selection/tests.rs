use birchpad_core::Rope;
use birchpad_syntax::*;
use std::sync::atomic::AtomicBool;

fn parsed(language: &str, source: &str) -> Syntax {
    let text = Rope::from_str(source);
    let mut syntax = Syntax::new(config(by_id(language).unwrap()).unwrap());
    let tree = syntax.parse_job(text).run(&AtomicBool::new(false)).unwrap();
    syntax.install(tree);
    syntax
}

/// What expanding from `range` again and again selects, as text.
fn steps(syntax: &Syntax, source: &str, mut range: std::ops::Range<usize>) -> Vec<String> {
    let mut found = Vec::new();
    while let Some(next) = expand_selection(syntax, range.clone()) {
        assert!(
            next.start <= range.start && range.end <= next.end,
            "{next:?} holds {range:?}"
        );
        assert!(next.len() > range.len());
        found.push(source[next.clone()].to_owned());
        range = next;
    }
    found
}

#[test]
fn a_caret_grows_to_its_token_then_the_brackets_inside_and_out() {
    let source = "fn f() { g(\"hi\", a); }";
    let syntax = parsed("rust", source);
    let caret = source.find("hi").unwrap() + 1;
    let found = steps(&syntax, source, caret..caret);
    assert_eq!(found[..4], ["hi", "\"hi\"", "\"hi\", a", "(\"hi\", a)"]);
    assert!(found.contains(&"g(\"hi\", a)".to_owned()), "{found:?}");
    assert_eq!(found.last().map(String::as_str), Some(source));
}

#[test]
fn the_whole_tree_grows_no_further() {
    let source = "fn f() {}";
    let syntax = parsed("rust", source);
    assert_eq!(expand_selection(&syntax, 0..source.len()), None);
    // Past the end of the text: nothing holds it.
    assert_eq!(expand_selection(&syntax, 0..source.len() + 5), None);
}

#[test]
fn an_empty_document_has_nothing_to_select() {
    let syntax = parsed("rust", "");
    assert_eq!(expand_selection(&syntax, 0..0), None);
}

#[test]
fn embedded_languages_select_by_their_own_tree() {
    let source = "<script>let x = [1, 2];</script>";
    let syntax = parsed("html", source);
    let caret = source.find("2").unwrap();
    let found = steps(&syntax, source, caret..caret);
    assert_eq!(found[0], "2");
    assert!(found.contains(&"1, 2".to_owned()), "{found:?}");
    assert!(found.contains(&"[1, 2]".to_owned()), "{found:?}");
}
