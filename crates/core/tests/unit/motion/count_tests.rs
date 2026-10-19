use super::*;

#[test]
fn counts_of_a_text() {
    let counts = count_text(&Rope::from("Hello, wörld_1!\r\nсвет 2\n"));
    assert_eq!(
        counts,
        TextCounts {
            characters: 21,
            words: 4,
            lines: 3,
            bytes: 29,
        }
    );
    assert_eq!(
        count_text(&Rope::new()),
        TextCounts {
            characters: 0,
            words: 0,
            lines: 1,
            bytes: 0
        }
    );
    let one = count_text(&Rope::from("日本"));
    assert_eq!(
        (one.characters, one.words, one.lines, one.bytes),
        (2, 1, 1, 6)
    );
    let marks = count_text(&Rope::from("- . -\r"));
    assert_eq!((marks.characters, marks.words, marks.lines), (5, 0, 2));
}
