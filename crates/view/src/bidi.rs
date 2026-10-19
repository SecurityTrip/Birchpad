//! Right-to-left text in a row (ADR 0035): the Unicode Bidirectional Algorithm puts the
//! characters of a row in the order they show, and maps between the order they are stored in
//! (logical) and the order they show (visual), for carets, clicks and selections.

use std::ops::Range;

use unicode_bidi::{BidiClass, BidiInfo, Level, bidi_class};

/// A character of the row: where it is stored and where it shows, in bytes of the display text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Char {
    logical: usize,
    visual: usize,
    len: usize,
    rtl: bool,
}

/// A row whose characters show in another order than they are stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BidiRow {
    /// The text in the order it shows.
    pub visual: String,
    /// In logical order.
    chars: Vec<Char>,
    /// The paragraph runs right to left: the end of the text is at its left.
    base_rtl: bool,
}

fn is_rtl_class(class: BidiClass) -> bool {
    matches!(class, BidiClass::R | BidiClass::AL | BidiClass::AN)
}

impl BidiRow {
    /// The visual order of `text` (one row, no line breaks), with right to left as the base
    /// direction if `base_rtl`. `None` when it shows as stored: left to right, nothing to turn.
    pub fn new(text: &str, base_rtl: bool) -> Option<Self> {
        if !base_rtl && !text.chars().any(|ch| is_rtl_class(bidi_class(ch))) {
            return None;
        }
        let level = if base_rtl { Level::rtl() } else { Level::ltr() };
        let info = BidiInfo::new(text, Some(level));
        let mut chars: Vec<Char> = text
            .char_indices()
            .map(|(logical, ch)| Char {
                logical,
                visual: 0,
                len: ch.len_utf8(),
                rtl: false,
            })
            .collect();
        let mut visual = String::with_capacity(text.len());
        if let Some(paragraph) = info.paragraphs.first() {
            let line = paragraph.range.clone();
            let (levels, runs) = info.visual_runs(paragraph, line);
            for run in runs {
                let rtl = levels[run.start].is_rtl();
                let mut indexes: Vec<usize> = chars
                    .iter()
                    .enumerate()
                    .filter(|(_, ch)| run.contains(&ch.logical))
                    .map(|(index, _)| index)
                    .collect();
                if rtl {
                    indexes.reverse();
                }
                for index in indexes {
                    let ch = &mut chars[index];
                    ch.visual = visual.len();
                    ch.rtl = rtl;
                    visual.push_str(&text[ch.logical..ch.logical + ch.len]);
                }
            }
        }
        Some(Self {
            visual,
            chars,
            base_rtl,
        })
    }

    /// Where a caret before the character stored at `logical` shows, in bytes of the visual
    /// text: left of a left-to-right character, right of a right-to-left one. The end of the
    /// text is past its last character, on the side the character's direction runs to.
    pub fn visual_caret(&self, logical: usize) -> usize {
        match self.chars.iter().find(|ch| ch.logical >= logical) {
            Some(ch) if ch.rtl => ch.visual + ch.len,
            Some(ch) => ch.visual,
            None => match self.chars.last() {
                Some(last) if last.rtl => last.visual,
                Some(last) => last.visual + last.len,
                None if self.base_rtl => 0,
                None => self.visual.len(),
            },
        }
    }

    /// The caret position, in bytes of the stored text, for a click at `visual` (a boundary of
    /// the visual text).
    pub fn logical_caret(&self, visual: usize) -> usize {
        let end = self
            .chars
            .iter()
            .map(|ch| ch.logical + ch.len)
            .max()
            .unwrap_or(0);
        if let Some(ch) = self.chars.iter().find(|ch| ch.visual == visual) {
            return if ch.rtl {
                ch.logical + ch.len
            } else {
                ch.logical
            };
        }
        // Right of the last character shown.
        match self.chars.iter().find(|ch| ch.visual + ch.len == visual) {
            Some(ch) if ch.rtl => ch.logical,
            Some(ch) => ch.logical + ch.len,
            None => end,
        }
    }

    /// The pieces of the visual text the characters stored in `logical` show as, left to right:
    /// a selection over mixed directions shows as several.
    pub fn visual_spans(&self, logical: Range<usize>) -> Vec<Range<usize>> {
        let mut shown: Vec<Range<usize>> = self
            .chars
            .iter()
            .filter(|ch| logical.start <= ch.logical && ch.logical + ch.len <= logical.end)
            .map(|ch| ch.visual..ch.visual + ch.len)
            .collect();
        shown.sort_by_key(|span| span.start);
        let mut spans: Vec<Range<usize>> = Vec::new();
        for span in shown {
            match spans.last_mut() {
                Some(last) if last.end == span.start => last.end = span.end,
                _ => spans.push(span),
            }
        }
        spans
    }

    /// The stored range of each character in visual order, for coloring the shown text.
    pub fn visual_chars(&self) -> Vec<Range<usize>> {
        let mut by_visual: Vec<&Char> = self.chars.iter().collect();
        by_visual.sort_by_key(|ch| ch.visual);
        by_visual
            .into_iter()
            .map(|ch| ch.logical..ch.logical + ch.len)
            .collect()
    }
}

#[cfg(test)]
mod tests {
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
}

#[cfg(test)]
mod properties {
    use proptest::prelude::*;

    use super::*;

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
}
