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
