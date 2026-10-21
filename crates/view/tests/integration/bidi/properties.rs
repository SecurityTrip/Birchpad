use proptest::prelude::*;

use birchpad_view::*;
use std::ops::Range;

/// Latin, Hebrew, Arabic, digits, spaces and punctuation, mixed.
fn mixed() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        prop_oneof![
            Just('a'),
            Just('Z'),
            Just('\u{5D0}'),
            Just('\u{5E9}'),
            Just('\u{627}'),
            Just('\u{644}'),
            Just('1'),
            Just('\u{661}'),
            Just(' '),
            Just('.'),
            Just('('),
        ],
        0..40,
    )
    .prop_map(|chars| chars.into_iter().collect())
}

proptest! {
    #[test]
    fn characters_map_one_to_one(text in mixed(), base_rtl: bool) {
        let Some(row) = BidiRow::new(&text, base_rtl) else {
            // Shown as stored: only without right-to-left text.
            prop_assert!(!base_rtl);
            return Ok(());
        };
        // The same characters, each once.
        let mut shown: Vec<char> = row.visual.chars().collect();
        let mut stored: Vec<char> = text.chars().collect();
        shown.sort_unstable();
        stored.sort_unstable();
        prop_assert_eq!(shown, stored);
        let mut ranges = row.visual_chars();
        prop_assert_eq!(ranges.len(), text.chars().count());
        ranges.sort_by_key(|range| range.start);
        let stored: Vec<Range<usize>> = text
            .char_indices()
            .map(|(at, ch)| at..at + ch.len_utf8())
            .collect();
        prop_assert_eq!(ranges, stored);
        // Every caret shows on a boundary of the shown text, and every boundary of the shown
        // text is a caret of the stored one.
        for (at, _) in text.char_indices().chain([(text.len(), ' ')]) {
            let visual = row.visual_caret(at);
            prop_assert!(row.visual.is_char_boundary(visual), "{} -> {}", at, visual);
        }
        for (at, _) in row.visual.char_indices().chain([(row.visual.len(), ' ')]) {
            let logical = row.logical_caret(at);
            prop_assert!(text.is_char_boundary(logical), "{} -> {}", at, logical);
        }
    }

    #[test]
    fn carets_in_one_direction_go_there_and_back(
        letters in proptest::collection::vec(prop_oneof![Just('\u{5D0}'), Just('\u{5E9}')], 1..30),
    ) {
        let text: String = letters.into_iter().collect();
        let row = BidiRow::new(&text, true).expect("right to left");
        for (at, _) in text.char_indices().chain([(text.len(), ' ')]) {
            prop_assert_eq!(row.logical_caret(row.visual_caret(at)), at);
        }
    }
}
