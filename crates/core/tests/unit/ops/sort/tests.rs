use super::*;
use crate::ops::test_support::{parse, run, show};

const LF: LineEnding = LineEnding::Lf;

fn sort(marked: &str, key: SortKey, descending: bool) -> String {
    let (text, selection) = parse(marked);
    let transaction = sort_lines(&text, &selection, key, descending, LF).unwrap();
    show(&text, &selection, transaction)
}

#[test]
fn sorts_whole_document_or_selected_lines() {
    assert_eq!(
        sort("b\nA\na\nB", SortKey::Lexicographic, false),
        "|A\nB\na\nb"
    );
    assert_eq!(
        sort("b\nA\na\nB", SortKey::IgnoreCase, false),
        "|A\na\nb\nB"
    );
    assert_eq!(
        sort("b\nA\na\nB", SortKey::Lexicographic, true),
        "|b\na\nB\nA"
    );
    assert_eq!(
        sort("z\n[c\nb\na]\nz", SortKey::Lexicographic, false),
        "z\n[a\nb\nc]\nz"
    );
    assert_eq!(sort("ccc\na\nbb", SortKey::Length, false), "|a\nbb\nccc");
}

#[test]
fn sorts_numbers() {
    assert_eq!(
        sort("10\n9\n-1\n\n100 apples", SortKey::Integer, false),
        "|\n-1\n9\n10\n100 apples"
    );
    assert_eq!(
        sort("1,5\n1,25\n-0,5", SortKey::DecimalComma, false),
        "|-0,5\n1,25\n1,5"
    );
    assert_eq!(sort("1.5\n1.25", SortKey::DecimalDot, true), "|1.5\n1.25");
    let (text, selection) = parse("1\nx\n2");
    assert_eq!(
        sort_lines(&text, &selection, SortKey::Integer, false, LF).unwrap_err(),
        SortError { line: 2 }
    );
}

#[test]
fn sorts_by_the_columns_of_a_rectangle() {
    use crate::selection::Range;
    // name,age: sort by the age column (bytes 2..4 of every line but the header).
    let (text, _) = parse("b,30,x\na,04,y\nc,15,z");
    let block = Selection::new([Range::new(2, 4), Range::new(9, 11), Range::new(16, 18)], 2);
    let sorted = sort_lines_by_columns(&text, &block, SortKey::Integer, false, LF).unwrap();
    assert_eq!(
        show(&text, &block, sorted),
        "[a,04,y\nc,15,z\nb,30,x]",
        "whole lines move"
    );
    // Equal keys keep their order; a zero-width rectangle keys to the line end.
    let (text, _) = parse("1b\n2a\n3a");
    let thin = Selection::new([Range::point(1), Range::point(4), Range::point(7)], 0);
    let sorted = sort_lines_by_columns(&text, &thin, SortKey::Lexicographic, false, LF);
    assert_eq!(show(&text, &thin, sorted.unwrap()), "|2a\n3a\n1b");
    let error = sort_lines_by_columns(&text, &thin, SortKey::Integer, false, LF);
    assert_eq!(error.unwrap_err(), SortError { line: 1 });
}

#[test]
fn reverse_shuffle_and_remove() {
    assert_eq!(run("a\nb\nc", |t, s| reverse_lines(t, s, LF)), "|c\nb\na");
    let shuffled = run("1\n2\n3\n4\n5", |t, s| shuffle_lines(t, s, 7, LF));
    let mut lines: Vec<&str> = shuffled.trim_start_matches('|').lines().collect();
    lines.sort_unstable();
    assert_eq!(lines, ["1", "2", "3", "4", "5"], "a permutation");
    assert_eq!(
        run("a\nb\na\na\nc", |t, s| remove_duplicate_lines(
            t, s, false, LF
        )),
        "|a\nb\nc"
    );
    assert_eq!(
        run("a\nb\na\na\nc", |t, s| remove_duplicate_lines(
            t, s, true, LF
        )),
        "|a\nb\na\nc"
    );
    assert_eq!(
        run("a\n\n  \nb", |t, s| remove_empty_lines(t, s, false, LF)),
        "|a\n  \nb"
    );
    assert_eq!(
        run("a\n\n  \nb", |t, s| remove_empty_lines(t, s, true, LF)),
        "|a\nb"
    );
    assert_eq!(
        run("x", |t, s| reverse_lines(t, s, LF)),
        "|x",
        "nothing to do"
    );
}
