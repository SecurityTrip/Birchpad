use birchpad_core::ops::*;
use birchpad_core::*;

fn apply(text: &str, transaction: Option<Transaction>) -> String {
    let mut rope = Rope::from_str(text);
    if let Some(transaction) = transaction {
        transaction.changes().apply(&mut rope);
    }
    rope.to_string()
}

#[test]
fn copies_lines_and_ranges_one_per_line() {
    let text = Rope::from_str("one\r\ntwo\nthree");
    assert_eq!(copy_lines(&text, &[0, 2], LineEnding::Lf), "one\nthree\n");
    assert_eq!(copy_lines(&text, &[], LineEnding::Lf), "");
    assert_eq!(
        copy_ranges(&text, [9..14, 0..3, 9..14, 1..2], LineEnding::CrLf),
        "one\r\nn\r\nthree\r\n"
    );
}

#[test]
fn removes_bookmarked_lines_without_leaving_an_empty_line() {
    let text = "0\n1\n2\n3\n4";
    assert_eq!(
        apply(text, remove_lines(&Rope::from_str(text), &[1, 2, 4])),
        "0\n3"
    );
    assert_eq!(
        apply(text, remove_lines(&Rope::from_str(text), &[0])),
        "1\n2\n3\n4"
    );
    assert_eq!(
        apply(text, remove_lines(&Rope::from_str(text), &[0, 1, 2, 3, 4])),
        ""
    );
    assert!(remove_lines(&Rope::from_str(text), &[]).is_none());
    // The empty line after a final line break is a line too.
    assert_eq!(
        apply("a\nb\n", remove_lines(&Rope::from_str("a\nb\n"), &[2])),
        "a\nb"
    );
    assert_eq!(
        apply(
            "a\r\nb\r\n",
            remove_lines(&Rope::from_str("a\r\nb\r\n"), &[0])
        ),
        "b\r\n"
    );
}

#[test]
fn removes_unmarked_lines() {
    let text = "0\n1\n2\n3\n4";
    assert_eq!(
        apply(text, remove_other_lines(&Rope::from_str(text), &[1, 3])),
        "1\n3"
    );
    assert_eq!(
        apply(text, remove_other_lines(&Rope::from_str(text), &[4])),
        "4"
    );
    assert!(remove_other_lines(&Rope::from_str(text), &[0, 1, 2, 3, 4]).is_none());
}

#[test]
fn pastes_over_bookmarked_lines() {
    let text = "a\nb\n\nc";
    assert_eq!(
        apply(text, replace_lines(&Rope::from_str(text), &[0, 2, 3], "X")),
        "X\nb\nX\nX"
    );
    assert_eq!(
        apply(text, replace_lines(&Rope::from_str(text), &[1], "1\n2")),
        "a\n1\n2\n\nc"
    );
    assert!(replace_lines(&Rope::from_str(text), &[1], "b").is_none());
}

#[test]
fn inverse_takes_the_other_lines() {
    assert_eq!(other_lines(5, &[1, 3]), [0, 2, 4]);
    assert_eq!(other_lines(3, &[]), [0, 1, 2]);
    assert_eq!(other_lines(2, &[0, 1, 7]), Vec::<usize>::new());
}
