use crate::ops_test_support::{parse, show};
use birchpad_core::ops::*;
use birchpad_core::*;

fn run(
    marked: &str,
    options: MatchOptions,
    op: fn(&Rope, &Selection, MatchOptions) -> Option<Selection>,
) -> String {
    let (text, selection) = parse(marked);
    match op(&text, &selection, options) {
        Some(selection) => show(&text, &selection, None),
        None => "none".to_owned(),
    }
}

const ANY: MatchOptions = MatchOptions {
    match_case: false,
    whole_word: false,
};

#[test]
fn next_selects_the_word_then_adds_occurrences() {
    assert_eq!(
        run("fo|o bar Foo foo", ANY, select_next),
        "[foo] bar Foo foo"
    );
    assert_eq!(
        run("[foo] bar Foo foo", ANY, select_next),
        "[foo] bar [Foo] foo"
    );
    let case = MatchOptions {
        match_case: true,
        ..ANY
    };
    assert_eq!(
        run("[foo] bar Foo foo", case, select_next),
        "[foo] bar Foo [foo]"
    );
    assert_eq!(
        run("a | b", ANY, select_next),
        "none",
        "no word at the caret"
    );
}

#[test]
fn next_wraps_and_skips_selected_occurrences() {
    let (text, selection) = parse("ab [ab] ab");
    let second = select_next(&text, &selection, ANY).unwrap();
    assert_eq!(show(&text, &second, None), "ab [ab] [ab]");
    assert_eq!(second.primary(), Range::new(6, 8));
    let third = select_next(&text, &second, ANY).unwrap();
    assert_eq!(
        show(&text, &third, None),
        "[ab] [ab] [ab]",
        "wrapped around"
    );
    assert_eq!(third.primary(), Range::new(0, 2));
    assert!(select_next(&text, &third, ANY).is_none(), "all selected");
}

#[test]
fn whole_word_skips_parts_of_words() {
    let whole = MatchOptions {
        whole_word: true,
        ..ANY
    };
    assert_eq!(
        run("[cat] category cat", whole, select_next),
        "[cat] category [cat]"
    );
    assert_eq!(
        run("c|at category cat", whole, select_all),
        "[cat] category [cat]"
    );
    assert_eq!(
        run("c|at category cat", ANY, select_all),
        "[cat] [cat]egory [cat]"
    );
}

#[test]
fn select_all_keeps_the_primary_and_skip_moves_on() {
    let (text, selection) = parse("x y |x x");
    let all = select_all(&text, &selection, ANY).unwrap();
    assert_eq!(show(&text, &all, None), "[x] y [x] [x]");
    assert_eq!(all.primary(), Range::new(4, 5));
    assert_eq!(run("[x] y x x", ANY, skip_to_next), "x y [x] x");
    let (text, selection) = parse("[x] y [x] x");
    let skipped = skip_to_next(&text, &selection.with_primary_index(1), ANY).unwrap();
    assert_eq!(show(&text, &skipped, None), "[x] y x [x]");
    assert_eq!(run("[only] one", ANY, skip_to_next), "none");
}
