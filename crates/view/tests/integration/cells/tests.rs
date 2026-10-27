use birchpad_core::Rope;
use birchpad_view::*;

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

#[test]
fn unicode_line_breaks_show_as_pictures_one_cell_wide() {
    for ch in ['\u{85}', '\u{2028}', '\u{2029}'] {
        assert_eq!(char_cells(ch), 1, "{ch:?}");
    }
    assert_eq!(control_picture('\u{85}'), Some('␤'));
    assert_eq!(control_picture('\u{2028}'), Some('␤'));
    assert_eq!(control_picture('\u{2029}'), Some('¶'));
    assert_eq!(control_picture('\u{86}'), Some('\u{25AF}'));
    assert_eq!(control_picture('a'), None);
}

#[test]
fn control_characters_show_as_spaces_without_pictures() {
    let text = Rope::from_str("a\u{1}b\tc");
    let shown = DisplayText::with_pictures(&text, 0..text.len(), 0, 4, false);
    assert_eq!(shown.text, "a b c");
    let pictured = DisplayText::with_pictures(&text, 0..text.len(), 0, 4, true);
    assert_eq!(pictured.text, "a␁b c");
    assert_eq!(DisplayText::new(&text, 0..text.len(), 0, 4), pictured);
    // Positions map the same either way.
    assert_eq!(shown.to_display(2), 2);
}

#[test]
fn non_printing_characters_are_odd_spaces_or_marks() {
    for ch in ['\u{A0}', '\u{2009}', '\u{202F}', '\u{3000}'] {
        assert_eq!(non_printing(ch), Some(NonPrinting::Space), "{ch:?}");
    }
    for ch in ['\u{200B}', '\u{200D}', '\u{FEFF}', '\u{AD}', '\u{2066}'] {
        assert_eq!(non_printing(ch), Some(NonPrinting::Mark), "{ch:?}");
    }
    for ch in [' ', '\t', 'a', '\u{1}', '\u{2028}'] {
        assert_eq!(non_printing(ch), None, "{ch:?}");
    }
}

fn labeled(labels: NonPrintingLabel) -> Grid {
    Grid {
        tab_width: 4,
        labels,
    }
}

#[test]
fn non_printing_labels_by_abbreviation_or_code_point() {
    use NonPrintingLabel::*;
    assert_eq!(
        non_printing_label('\u{A0}', Abbreviation).as_deref(),
        Some("NBSP")
    );
    assert_eq!(
        non_printing_label('\u{A0}', CodePoint).as_deref(),
        Some("U+00A0")
    );
    assert_eq!(
        non_printing_label('\u{200B}', Abbreviation).as_deref(),
        Some("ZWSP")
    );
    // Without an abbreviation, the code point.
    assert_eq!(
        non_printing_label('\u{2061}', Abbreviation).as_deref(),
        Some("U+2061")
    );
    assert_eq!(
        non_printing_label('\u{1680}', Abbreviation).as_deref(),
        Some("U+1680")
    );
}

#[test]
fn non_printing_labels_off_or_for_other_characters_are_none() {
    use NonPrintingLabel::*;
    assert_eq!(non_printing_label('\u{A0}', Off), None);
    for ch in ['a', ' ', '\t', '\0', '\u{2028}', '日'] {
        assert_eq!(non_printing_label(ch, Abbreviation), None, "{ch:?}");
        assert_eq!(non_printing_label(ch, CodePoint), None, "{ch:?}");
    }
    assert_eq!(abbreviation('a'), None);
}

#[test]
fn non_printing_labels_at_the_ends_of_their_ranges() {
    use NonPrintingLabel::*;
    for (ch, short) in [
        ('\u{2000}', "NQSP"),
        ('\u{200A}', "HSP"),
        ('\u{202F}', "NNBSP"),
        ('\u{3000}', "IDSP"),
        ('\u{FEFF}', "BOM"),
    ] {
        assert_eq!(non_printing_label(ch, Abbreviation).as_deref(), Some(short));
    }
    assert_eq!(
        non_printing_label('\u{2064}', Abbreviation).as_deref(),
        Some("U+2064")
    );
    assert_eq!(
        non_printing_label('\u{FEFF}', CodePoint).as_deref(),
        Some("U+FEFF")
    );
    // Just past the ranges.
    for ch in ['\u{9F}', '\u{1FFF}', '\u{2065}', '\u{3001}', '\u{FF00}'] {
        assert_eq!(non_printing_label(ch, CodePoint), None, "{ch:?}");
    }
}

#[test]
fn labeled_characters_take_the_cells_of_their_label() {
    use NonPrintingLabel::*;
    for ch in ['\u{A0}', '\u{200B}', '\u{FEFF}'] {
        let label = non_printing_label(ch, Abbreviation).expect("labeled");
        assert_eq!(
            cells_at(ch, 0, labeled(Abbreviation)),
            label.len(),
            "{ch:?}"
        );
        assert_eq!(cells_at(ch, 3, labeled(CodePoint)), 6, "{ch:?}");
    }
    // Others keep their width, tabs their stops.
    assert_eq!(cells_at('a', 0, labeled(CodePoint)), 1);
    assert_eq!(cells_at('日', 0, labeled(CodePoint)), 2);
    assert_eq!(cells_at('\t', 1, labeled(CodePoint)), 3);
    // Without labels, as before: a framed space one cell, a mark none.
    assert_eq!(cells_at('\u{A0}', 0, 4), 1);
    assert_eq!(cells_at('\u{200B}', 0, labeled(Off)), 0);
    assert!(labeled(Off).cells_within_bytes());
    assert!(!labeled(Abbreviation).cells_within_bytes());
}

#[test]
fn display_text_draws_labels_and_maps_through_them() {
    let text = Rope::from_str("a\u{A0}b");
    let display = DisplayText::new(
        &text,
        0..text.len(),
        0,
        labeled(NonPrintingLabel::Abbreviation),
    );
    assert_eq!(display.text, "aNBSPb");
    assert_eq!(display.to_display(1), 1);
    assert_eq!(display.to_display(3), 5, "b after the label");
    assert_eq!(display.to_doc(2), 1, "the label's left half");
    assert_eq!(display.to_doc(4), 3, "its right half");
    assert_eq!(display.to_doc(6), 4);
    let chars: Vec<_> = display.chars(&text).collect();
    assert_eq!(chars[1], (1, '\u{A0}', 1..5));
    // Without labels the character stays.
    let plain = DisplayText::new(&text, 0..text.len(), 0, 4);
    assert_eq!(plain.text, text.to_string());
}

#[test]
fn a_label_as_long_as_its_character_still_maps_to_its_ends() {
    // ZWJ is three bytes in UTF-8, and so is "ZWJ": an offset inside the label is still not
    // one of the document's.
    let text = Rope::from_str("a\u{200D}b");
    let display = DisplayText::new(
        &text,
        0..text.len(),
        0,
        labeled(NonPrintingLabel::Abbreviation),
    );
    assert_eq!(display.text, "aZWJb");
    assert_eq!(display.to_doc(2), 1);
    assert_eq!(display.to_doc(3), 4);
    assert_eq!(display.to_display(4), 4);
}

#[test]
fn columns_count_labels() {
    let text = Rope::from_str("a\u{A0}b\u{200B}c");
    let grid = labeled(NonPrintingLabel::CodePoint);
    assert_eq!(
        column_after(&text, 0, 0, text.len(), grid),
        1 + 6 + 1 + 6 + 1
    );
    // The zero-width space takes no cell of its own.
    assert_eq!(column_after(&text, 0, 0, text.len(), 4), 4);
    // A column inside the label rounds to its nearer end.
    assert_eq!(pos_at_column(&text, 0..text.len(), 0, 3, grid), (1, 1));
    assert_eq!(pos_at_column(&text, 0..text.len(), 0, 5, grid), (3, 7));
    // Empty text.
    let empty = Rope::from_str("");
    assert_eq!(column_after(&empty, 0, 0, 0, grid), 0);
}
