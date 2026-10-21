use ropey::Rope;

#[test]
fn characters_in_a_range() {
    use birchpad_core::search::{Direction::*, find_char_in_range};
    let text = Rope::from("aé	b€");
    let non_ascii = 128..=0x10_FFFF;
    assert_eq!(
        find_char_in_range(&text, 0, non_ascii.clone(), Forward, false),
        Some(1..3)
    );
    assert_eq!(
        find_char_in_range(&text, 3, non_ascii.clone(), Forward, false),
        Some(5..8)
    );
    assert_eq!(
        find_char_in_range(&text, 8, non_ascii.clone(), Forward, false),
        None
    );
    assert_eq!(
        find_char_in_range(&text, 8, non_ascii.clone(), Forward, true),
        Some(1..3)
    );
    assert_eq!(
        find_char_in_range(&text, 5, non_ascii.clone(), Backward, false),
        Some(1..3)
    );
    assert_eq!(
        find_char_in_range(&text, 1, non_ascii.clone(), Backward, false),
        None
    );
    assert_eq!(
        find_char_in_range(&text, 1, non_ascii, Backward, true),
        Some(5..8)
    );
    // Control characters; one code; an empty range; inside a character; empty text.
    assert_eq!(
        find_char_in_range(&text, 0, 0..=31, Forward, false),
        Some(3..4)
    );
    assert_eq!(
        find_char_in_range(&text, 0, 98..=98, Forward, false),
        Some(4..5)
    );
    #[allow(clippy::reversed_empty_ranges)]
    let empty = 5..=4;
    assert_eq!(find_char_in_range(&text, 0, empty, Forward, true), None);
    assert_eq!(
        find_char_in_range(&text, 2, 0..=127, Forward, false),
        Some(3..4)
    );
    assert_eq!(
        find_char_in_range(&Rope::new(), 0, 0..=0x10_FFFF, Backward, true),
        None
    );
    assert_eq!(
        find_char_in_range(&text, 99, 97..=97, Backward, false),
        Some(0..1)
    );
}
