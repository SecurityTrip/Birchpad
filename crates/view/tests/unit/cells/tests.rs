use super::*;

#[test]
fn control_pictures_at_the_ends_of_their_ranges() {
    // C0 controls, except the tab, have pictures from U+2400 on.
    assert_eq!(control_picture('\0'), Some('\u{2400}'));
    assert_eq!(control_picture('\u{1B}'), Some('\u{241B}'));
    assert_eq!(control_picture('\u{1F}'), Some('\u{241F}'));
    assert_eq!(control_picture('\t'), None);
    // DEL has its own; the C1 controls a box.
    assert_eq!(control_picture('\u{7F}'), Some('\u{2421}'));
    assert_eq!(control_picture('\u{80}'), Some('\u{25AF}'));
    assert_eq!(control_picture('\u{9F}'), Some('\u{25AF}'));
    // Just past each range, and ordinary characters, have none.
    for ch in [' ', '~', '\u{A0}', 'a', 'Ж'] {
        assert_eq!(control_picture(ch), None, "{ch:?}");
    }
}

#[test]
fn widths() {
    assert_eq!(char_cells('a'), 1);
    assert_eq!(char_cells('ж'), 1);
    assert_eq!(char_cells('日'), 2);
    assert_eq!(char_cells('😀'), 2);
    assert_eq!(char_cells('\u{301}'), 0, "combining acute accent");
    assert_eq!(char_cells('\0'), 1);
    assert_eq!(tab_advance(0, 4), 4);
    assert_eq!(tab_advance(5, 4), 3);
    assert_eq!(tab_advance(8, 4), 4);
}

#[test]
fn display_text_expands_tabs_to_the_next_stop() {
    let text = Rope::from_str("a\tbc\td\0");
    let display = DisplayText::new(&text, 0..text.len(), 0, 4);
    assert_eq!(display.text, "a   bc  d\u{2400}");
    assert_eq!(display.to_display(0), 0);
    assert_eq!(display.to_display(1), 1, "the tab starts at 1");
    assert_eq!(display.to_display(2), 4, "b after the tab");
    assert_eq!(display.to_display(5), 8, "d");
    assert_eq!(display.to_display(6), 9, "NUL");
    assert_eq!(display.to_display(7), 12, "end");
    // Inside the expanded tab: the nearer end.
    assert_eq!(display.to_doc(2), 1);
    assert_eq!(display.to_doc(3), 2);
    assert_eq!(display.to_doc(4), 2);
    assert_eq!(display.to_doc(12), 7);
    assert_eq!(display.doc_end(), 7);
}

#[test]
fn display_text_characters_in_one_pass() {
    let text = Rope::from_str("xa\tж\0b");
    let display = DisplayText::new(&text, 1..text.len(), 1, 4);
    let chars: Vec<_> = display.chars(&text).collect();
    assert_eq!(
        chars,
        [
            (1, 'a', 0..1),
            (2, '\t', 1..3),
            (3, 'ж', 3..5),
            (5, '\0', 5..8),
            (6, 'b', 8..9)
        ]
    );
    for (pos, _, range) in chars {
        assert_eq!(display.to_display(pos), range.start);
    }
}

#[test]
fn display_text_of_a_partial_row_keeps_tab_stops() {
    let text = Rope::from_str("abcdef\tg");
    // The row starts at "ef", which is column 4.
    let display = DisplayText::new(&text, 4..8, 4, 4);
    assert_eq!(display.text, "ef  g");
    assert_eq!(display.to_display(7), 4);
}

#[test]
fn columns_and_hit_testing() {
    let text = Rope::from_str("\tж日x");
    assert_eq!(column_after(&text, 0, 0, 1, 4), 4);
    assert_eq!(column_after(&text, 0, 0, 3, 4), 5);
    assert_eq!(column_after(&text, 0, 0, 6, 4), 7);
    let all = 0..text.len();
    assert_eq!(
        pos_at_column(&text, all.clone(), 0, 1, 4),
        (0, 0),
        "left half of the tab"
    );
    assert_eq!(
        pos_at_column(&text, all.clone(), 0, 3, 4),
        (1, 4),
        "right half"
    );
    assert_eq!(
        pos_at_column(&text, all.clone(), 0, 6, 4),
        (6, 7),
        "right half of 日"
    );
    assert_eq!(
        pos_at_column(&text, all.clone(), 0, 99, 4),
        (7, 8),
        "past the end"
    );
}
