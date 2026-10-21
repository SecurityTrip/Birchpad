use birchpad_core::*;

#[test]
fn typing_with_several_carets() {
    let mut text = Rope::from_str("ab\ncd\nef");
    let selection = Selection::new([Range::point(0), Range::point(3), Range::new(6, 8)], 2);
    let transaction = Transaction::replace_selections(&text, &selection, "> ").unwrap();
    transaction.changes().apply(&mut text);

    assert_eq!(text, "> ab\n> cd\n> ");
    let carets = transaction.selection().unwrap();
    assert_eq!(
        carets.ranges(),
        [Range::point(2), Range::point(7), Range::point(12)]
    );
    assert_eq!(carets.primary_index(), 2);
}

#[test]
fn each_caret_can_insert_different_text() {
    let mut text = Rope::from_str("a\nbcd");
    let selection = Selection::new([Range::point(1), Range::point(5)], 0);
    let transaction =
        Transaction::replace_selections_with(&text, &selection, |range| " ".repeat(range.head % 3))
            .unwrap();
    transaction.changes().apply(&mut text);
    assert_eq!(text, "a \nbcd  ");
    assert_eq!(
        transaction.selection().unwrap().ranges(),
        [Range::point(2), Range::point(8)]
    );
}

#[test]
fn typing_in_virtual_space_fills_it_with_spaces() {
    let mut text = Rope::from_str("abcd\nx\n");
    // A rectangle from column 2 to 3: "c" on the first line, virtual space on the second
    // (whose end is at byte 6), and an empty line in virtual space.
    let selection = Selection::new(
        [
            Range::new(2, 3),
            Range::new(6, 6).with_virtual(1, 2),
            Range::new(7, 7).with_virtual(2, 3),
        ],
        0,
    );
    let typed = Transaction::replace_selections(&text, &selection, "Z").unwrap();
    let mut typed_text = text.clone();
    typed.changes().apply(&mut typed_text);
    assert_eq!(typed_text, "abZd\nx Z\n  Z");
    assert_eq!(
        typed.selection().unwrap().ranges(),
        [Range::point(3), Range::point(8), Range::point(12)]
    );

    // Deleting keeps the carets at the left edge, in virtual space where it was.
    let deleted = Transaction::replace_selections(&text, &selection, "").unwrap();
    deleted.changes().apply(&mut text);
    assert_eq!(text, "abd\nx\n");
    assert_eq!(
        deleted.selection().unwrap().ranges(),
        [
            Range::point(2),
            Range::virtual_point(5, 1),
            Range::virtual_point(6, 2)
        ]
    );
}

#[test]
fn deleting_adjacent_ranges_merges_carets() {
    let mut text = Rope::from_str("abcd");
    let selection = Selection::new([Range::new(0, 2), Range::new(2, 4)], 0);
    let transaction = Transaction::replace_selections(&text, &selection, "").unwrap();
    transaction.changes().apply(&mut text);

    assert_eq!(text, "");
    assert_eq!(transaction.selection().unwrap().ranges(), [Range::point(0)]);
}
