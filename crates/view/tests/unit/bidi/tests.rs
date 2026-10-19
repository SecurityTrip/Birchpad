use super::*;

#[test]
fn left_to_right_text_needs_nothing() {
    assert_eq!(BidiRow::new("plain text", false), None);
    assert_eq!(BidiRow::new("", false), None);
    assert!(BidiRow::new("", true).is_some());
}

#[test]
fn hebrew_shows_reversed() {
    // אבג stored, גבא shown.
    let row = BidiRow::new("אבג", false).unwrap();
    assert_eq!(row.visual, "גבא");
    // Before the first character: at its right, the right end of the text.
    assert_eq!(row.visual_caret(0), 6);
    assert_eq!(row.visual_caret(2), 4);
    assert_eq!(
        row.visual_caret(6),
        0,
        "the end of a Hebrew word is at its left"
    );
    assert_eq!(row.logical_caret(6), 0);
    assert_eq!(row.logical_caret(0), 6);
    assert_eq!(row.logical_caret(2), 4);
}

#[test]
fn mixed_text_keeps_each_run_in_its_order() {
    let text = "ab אב cd";
    let row = BidiRow::new(text, false).unwrap();
    assert_eq!(row.visual, "ab בא cd");
    let shown: String = row
        .visual_chars()
        .into_iter()
        .map(|range| &text[range])
        .collect();
    assert_eq!(shown, row.visual);
    // Each caret maps back to where it came from.
    for (logical, _) in text.char_indices() {
        let visual = row.visual_caret(logical);
        let back = row.logical_caret(visual);
        assert!(text.is_char_boundary(back));
    }
}

#[test]
fn selections_over_mixed_text_are_pieces() {
    // "ab אב cd": the Hebrew pair shows reversed in the middle.
    let text = "ab \u{5d0}\u{5d1} cd";
    let row = BidiRow::new(text, false).unwrap();
    let hebrew_start = text.find('\u{5d0}').unwrap();
    // From "b" into the Hebrew word: "b" on the left, the first Hebrew letter at the right
    // of the pair.
    let spans = row.visual_spans(1..hebrew_start + 2);
    assert_eq!(spans.len(), 2, "{spans:?}");
    assert_eq!(spans[0], 1..hebrew_start, "b and the space");
    // All of it is one piece; nothing is none.
    let whole = row.visual_spans(0..text.len());
    assert_eq!(whole.len(), 1);
    assert_eq!(whole[0], 0..text.len());
    assert!(row.visual_spans(2..2).is_empty());
    assert!(row.visual_spans(text.len()..text.len() + 5).is_empty());
}

#[test]
fn a_right_to_left_paragraph() {
    // Latin alone is one left-to-right run, also in a right-to-left paragraph.
    let row = BidiRow::new("ab cd", true).unwrap();
    assert_eq!(row.visual, "ab cd");
    assert_eq!(row.visual_caret(5), 5);
    assert_eq!(row.visual_caret(0), 0);
    // Hebrew and a number: the number keeps its digits' order, after the Hebrew word.
    let row = BidiRow::new("אב 12", true).unwrap();
    assert_eq!(row.visual, "12 בא");
    let empty = BidiRow::new("", true).unwrap();
    assert_eq!((empty.visual_caret(0), empty.logical_caret(0)), (0, 0));
}
