#![allow(
    clippy::single_range_in_vec_init,
    reason = "expected ranges written out"
)]

use birchpad_tools::compare::*;
use std::ops::Range;

fn diff(old: &str, new: &str) -> Vec<(Change, Range<usize>, Range<usize>)> {
    compare(old, new, Options::default())
        .into_iter()
        .map(|d| (d.change, d.old, d.new))
        .collect()
}

#[test]
fn finds_added_removed_and_changed_lines() {
    assert_eq!(
        diff("a\nb\nc\nd\n", "a\nx\nc\nd\ne\n"),
        [(Change::Changed, 1..2, 1..2), (Change::Added, 4..4, 4..5)]
    );
    assert_eq!(diff("a\nb\nc", "a\nc"), [(Change::Removed, 1..2, 1..1)]);
    assert_eq!(diff("b\nc", "a\nb\nc"), [(Change::Added, 0..0, 0..1)]);
}

#[test]
fn same_texts_have_no_differences() {
    assert!(diff("", "").is_empty());
    assert!(diff("a\r\nb\r\n", "a\nb\n").is_empty());
    assert!(diff("x", "x").is_empty());
}

#[test]
fn an_empty_text_against_another() {
    assert_eq!(diff("", "a\nb"), [(Change::Added, 0..0, 0..2)]);
    assert_eq!(diff("a\nb\n", ""), [(Change::Removed, 0..2, 0..0)]);
    // A final line break alone makes no line.
    assert_eq!(diff("a", "a\n"), []);
    assert_eq!(diff("a\n", "a\n\n"), [(Change::Added, 1..1, 1..2)]);
}

#[test]
fn can_ignore_whitespace_and_case() {
    let old = "if (a)\n\treturn B;\n";
    let new = "if ( a )\n    return b;\n";
    assert_eq!(compare(old, new, Options::default()).len(), 1);
    let loose = Options {
        ignore_whitespace: true,
        ignore_case: false,
    };
    assert_eq!(compare(old, new, loose).len(), 1);
    let looser = Options {
        ignore_whitespace: true,
        ignore_case: true,
    };
    assert!(compare(old, new, looser).is_empty());
}

#[test]
fn moves_between_differences_wrapping_around() {
    let differences = compare("a\nb\nc\nd\ne", "a\nB\nc\nD\ne", Options::default());
    assert_eq!(differences.len(), 2);
    assert_eq!(next(&differences, 0, true).unwrap().new, 1..2);
    assert_eq!(next(&differences, 1, true).unwrap().new, 3..4);
    assert_eq!(next(&differences, 3, true).unwrap().new, 1..2);
    assert_eq!(next(&differences, 4, false).unwrap().new, 3..4);
    assert_eq!(next(&differences, 1, false).unwrap().new, 3..4);
    assert_eq!(next(&[], 0, true), None);
    assert_eq!(next(&[], 0, false), None);
}

#[test]
fn splits_lines_without_their_breaks() {
    assert_eq!(lines(""), Vec::<&str>::new());
    assert_eq!(lines("\n"), [""]);
    assert_eq!(lines("a\r\nb"), ["a", "b"]);
    assert_eq!(lines("a\rb"), ["a\rb"]);
}

#[test]
fn pads_the_side_with_fewer_lines() {
    let pad = |old: &str, new: &str| padding(&compare(old, new, Options::default()));
    // Removed lines: blank rows in the new text where they were.
    assert_eq!(pad("a\nb\nc\n", "a\nc\n"), [vec![], vec![(1, 1)]]);
    // Added lines: in the old text.
    assert_eq!(pad("a\n", "a\nb\nc\n"), [vec![(1, 2)], vec![]]);
    // A change of two lines into three, and the start of the text.
    assert_eq!(pad("x\ny\nz", "1\n2\n3\nz"), [vec![(2, 1)], vec![]]);
    assert_eq!(pad("b", "a\nb"), [vec![(0, 1)], vec![]]);
    // The same, or as many lines either side: no padding.
    assert_eq!(pad("a\nb", "a\nb"), [vec![], vec![]]);
    assert_eq!(pad("a\nb", "x\ny"), [vec![], vec![]]);
    assert_eq!(pad("", ""), [vec![], vec![]]);
    assert_eq!(pad("", "a\nb"), [vec![(0, 2)], vec![]]);
}

#[test]
fn finds_what_differs_within_a_line() {
    let (old, new) = inline("let x = 1;", "let y = 12;", Options::default()).unwrap();
    assert_eq!(old, [4..5]);
    assert_eq!(new, [4..5, 9..10]);
    // Bytes, not characters.
    let (old, new) = inline("мир", "мор", Options::default()).unwrap();
    assert_eq!((old, new), (vec![2..4], vec![2..4]));
    // Only added or only removed on one side.
    let (old, new) = inline("ab", "abc", Options::default()).unwrap();
    assert_eq!((old, new), (vec![], vec![2..3]));
    // Adjacent characters make one range.
    let (old, _) = inline("abcd", "axyd", Options::default()).unwrap();
    assert_eq!(old, [1..3]);
}

#[test]
fn within_a_line_follows_the_options() {
    let ignoring = Options {
        ignore_whitespace: true,
        ignore_case: true,
    };
    let (old, new) = inline("If (A)", "if(a) ", ignoring).unwrap();
    assert!(old.is_empty() && new.is_empty());
    let (old, new) = inline("If (A)", "if(a) ", Options::default()).unwrap();
    assert!(!old.is_empty() && !new.is_empty());
}

#[test]
fn nothing_within_lines_too_different_or_too_long() {
    assert_eq!(inline("abc", "xyz", Options::default()), None);
    assert_eq!(inline("", "x", Options::default()), None);
    assert_eq!(inline("", "", Options::default()), None);
    let long = "a".repeat(MAX_INLINE + 1);
    assert_eq!(inline(&long, "a", Options::default()), None);
    let longest = "a".repeat(MAX_INLINE);
    assert!(inline(&longest, "a", Options::default()).is_some());
}

#[test]
fn finds_inline_differences_of_the_changed_lines() {
    let old = "same\nlet a = 1;\r\nfoo\nend\n";
    let new = "same\nlet b = 1;\nfoo bar\nextra\nend\n";
    let differences = compare(old, new, Options::default());
    let [old_ranges, new_ranges] = inline_differences(old, new, &differences, Options::default());
    // "a" of line 1, at its byte offset in the whole text; line 2's change adds " bar".
    assert_eq!(old_ranges, [9..10]);
    assert_eq!(&new[new_ranges[0].clone()], "b");
    assert_eq!(&new[new_ranges[1].clone()], " bar");
    // Added and removed lines, and identical texts, have none.
    let added = compare("a\n", "a\nb\n", Options::default());
    let none: [Vec<Range<usize>>; 2] = [Vec::new(), Vec::new()];
    assert_eq!(
        inline_differences("a\n", "a\nb\n", &added, Options::default()),
        none
    );
    assert_eq!(inline_differences("", "", &[], Options::default()), none);
}
