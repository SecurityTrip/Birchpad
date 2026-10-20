use birchpad_core::motion::*;
use ropey::Rope;

#[test]
fn word_movement_like_scintilla() {
    let text = Rope::from_str("let x = foo(bar);  // ok\nnext");
    let stops_right: Vec<usize> = std::iter::successors(Some(0), |&p| {
        let next = word_right(&text, p);
        (next != p).then_some(next)
    })
    .collect();
    // let|x|=|foo|(|bar|);|//|ok|\n|next
    assert_eq!(stops_right, [0, 4, 6, 8, 11, 12, 15, 19, 22, 24, 25, 29]);
    let stops_left: Vec<usize> = std::iter::successors(Some(text.len()), |&p| {
        let next = word_left(&text, p);
        (next != p).then_some(next)
    })
    .collect();
    assert_eq!(stops_left, [29, 25, 24, 22, 19, 15, 12, 11, 8, 6, 4, 0]);
}

#[test]
fn word_movement_handles_unicode_and_crlf() {
    let text = Rope::from_str("привет, мир\r\n  日本");
    assert_eq!(word_right(&text, 0), 12, "to the comma");
    assert_eq!(word_right(&text, 12), 14);
    assert_eq!(word_right(&text, 20), 22, "a line break is one stop");
    assert_eq!(word_left(&text, 22), 20);
    assert_eq!(word_left(&text, 30), 24, "back over spaces to the word");
}

#[test]
fn double_click_selects_runs() {
    let text = Rope::from_str("foo_bar  (baz)");
    assert_eq!(word_at(&text, 2), 0..7);
    assert_eq!(word_at(&text, 7), 0..7, "end of word: the word before");
    assert_eq!(word_at(&text, 8), 7..9, "spaces");
    assert_eq!(word_at(&text, 9), 9..10, "punctuation");
    assert_eq!(word_at(&Rope::from_str(""), 0), 0..0);
}

#[test]
fn indentation() {
    let text = Rope::from_str("  \tcode\n    \nx");
    assert_eq!(indent_end(&text, 0), 3);
    assert_eq!(indent_end(&text, 1), 12, "blank line: its end");
    assert_eq!(indent_end(&text, 2), 13);
}

#[test]
fn line_ranges_exclude_breaks() {
    let text = Rope::from_str("ab\r\ncd\ne\rf");
    assert_eq!(line_count(&text), 4);
    assert_eq!(line_range(&text, 0), 0..2);
    assert_eq!(line_range_with_break(&text, 0), 0..4);
    assert_eq!(line_range(&text, 1), 4..6);
    assert_eq!(line_range(&text, 2), 7..8);
    assert_eq!(line_range(&text, 3), 9..10);
    assert_eq!(line_of(&text, 3), 0, "inside CRLF belongs to the line");
    assert_eq!(line_of(&text, 4), 1);
}

#[test]
fn crlf_is_one_step() {
    let text = Rope::from_str("a\r\nж");
    assert_eq!(next_boundary(&text, 1), 3);
    assert_eq!(prev_boundary(&text, 3), 1);
    assert_eq!(next_boundary(&text, 3), 5);
    assert_eq!(prev_boundary(&text, 5), 3);
    assert_eq!(next_boundary(&text, 5), 5);
    assert_eq!(prev_boundary(&text, 0), 0);
}
