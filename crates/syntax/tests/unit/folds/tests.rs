use super::*;
use crate::language::by_id;
use crate::syntax::{Syntax, config};
use std::sync::atomic::AtomicBool;

fn describe(text: &Rope, source: &str, folds: Vec<Range<usize>>) -> Vec<String> {
    folds
        .into_iter()
        .map(|range| {
            let first = source[range.clone()].lines().next().unwrap_or_default();
            let last = line_of(text, last_char(text, range.end));
            format!("{}..{} {first}", line_of(text, range.start), last)
        })
        .collect()
}

fn folds(language: &str, source: &str) -> Vec<String> {
    let text = Rope::from_str(source);
    let mut syntax = Syntax::new(config(by_id(language).unwrap()).unwrap());
    let tree = syntax
        .parse_job(text.clone())
        .run(&AtomicBool::new(false))
        .unwrap();
    syntax.install(tree);
    describe(&text, source, fold_ranges(syntax.tree().unwrap(), &text))
}

#[test]
fn multi_line_nodes_fold_once_per_line() {
    let source = "\
/* a
   comment */
fn f() {
    if x {
        y();
    }
    let s = [
        1,
    ];
}
fn g() {}
";
    assert_eq!(
        folds("rust", source),
        [
            "0..1 /* a",
            "2..9 fn f() {",
            "3..5 if x {",
            "6..8 let s = [",
        ]
    );
}

#[test]
fn bodies_without_an_opening_token_do_not_fold_their_first_line() {
    let source =
        "def f():\n    x = 1\n    if x:\n        pass\n    return 1\n\nclass C:\n    pass\n";
    assert_eq!(
        folds("python", source),
        ["0..4 def f():", "2..3 if x:", "6..7 class C:"]
    );
    let yaml = "jobs:\n  test:\n    runs-on: x\n    steps: 1\n  other: 2\n";
    assert_eq!(folds("yaml", yaml), ["0..4 jobs:", "1..3 test:"]);
}

#[test]
fn markup_folds_elements_and_sections() {
    let html = "<ul>\n  <li>a</li>\n  <li>\n    b\n  </li>\n</ul>\n";
    assert_eq!(folds("html", html), ["0..5 <ul>", "2..4 <li>"]);
    let markdown = "# One\n\ntext\n\n## Two\n\nmore\n";
    assert_eq!(folds("markdown", markdown), ["0..6 # One", "4..6 ## Two"]);
}

#[test]
fn plain_text_folds_by_indentation() {
    let source = "a\n  b\n\n    c\n  d\ne\n\n  f\n";
    let text = Rope::from_str(source);
    assert_eq!(
        describe(&text, source, indent_fold_ranges(&text, 4)),
        ["0..4 a", "1..3   b", "5..7 e"]
    );
}
