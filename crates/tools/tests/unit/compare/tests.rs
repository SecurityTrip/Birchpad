use super::*;

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
