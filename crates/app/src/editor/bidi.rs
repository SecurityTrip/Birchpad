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
pub(crate) struct BidiRow {
    /// The text in the order it shows.
    pub(crate) visual: String,
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
    pub(crate) fn new(text: &str, base_rtl: bool) -> Option<Self> {
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
    pub(crate) fn visual_caret(&self, logical: usize) -> usize {
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
    pub(crate) fn logical_caret(&self, visual: usize) -> usize {
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

    /// The stored range of each character in visual order, for coloring the shown text.
    pub(crate) fn visual_chars(&self) -> Vec<Range<usize>> {
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

    fn editor(
        cx: &mut gpui_kit::TestAppContext,
    ) -> (
        gpui_kit::Entity<crate::workspace::Workspace>,
        &mut gpui_kit::VisualTestContext,
    ) {
        crate::workspace::tests::open_workspace(cx)
    }

    #[gpui_kit::test]
    fn hebrew_rows_show_turned_and_carets_follow(cx: &mut gpui_kit::TestAppContext) {
        let (workspace, cx) = editor(cx);
        cx.simulate_input("אבג");
        cx.run_until_parked();
        let view = workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap());
        view.read_with(cx, |view, _| {
            let layout = view.layout.as_ref().expect("a frame was drawn");
            let row = &layout.rows[0];
            assert_eq!(
                row.bidi.as_ref().map(|bidi| bidi.visual.as_str()),
                Some("גבא")
            );
            // The start of the word shows right of its end.
            assert!(row.x_for(0).unwrap() > row.x_for(6).unwrap());
        });
        // Clicking at the left end puts the caret after the word.
        cx.simulate_keystrokes("home");
        let at_left = view.read_with(cx, |view, _| {
            let layout = view.layout.as_ref().unwrap();
            let row = &layout.rows[0];
            gpui_kit::point(
                row.x + gpui_kit::px(1.),
                row.y + layout.metrics.line_height / 2.,
            )
        });
        cx.simulate_click(at_left, gpui_kit::Modifiers::none());
        assert_eq!(
            view.read_with(cx, |view, _| view.selection.primary().head),
            6
        );
    }

    #[gpui_kit::test]
    fn right_to_left_rows_end_at_the_right_edge(cx: &mut gpui_kit::TestAppContext) {
        use birchpad_commands::Invocation;
        let (workspace, cx) = editor(cx);
        cx.simulate_input("ab");
        let view = workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap());
        let left = view.read_with(cx, |view, _| view.layout.as_ref().unwrap().rows[0].x);
        workspace.update_in(cx, |workspace, window, cx| {
            workspace
                .dispatch(&Invocation::new("edit.text-direction-rtl"), window, cx)
                .unwrap();
        });
        cx.run_until_parked();
        let (right, edge) = view.read_with(cx, |view, _| {
            let layout = view.layout.as_ref().unwrap();
            let row = &layout.rows[0];
            (row.x + row.shaped.width(), layout.text_bounds.right())
        });
        assert!(
            right > left + gpui_kit::px(100.) && right <= edge,
            "{right:?} {edge:?}"
        );
        workspace.update_in(cx, |workspace, window, cx| {
            workspace
                .dispatch(&Invocation::new("edit.text-direction-ltr"), window, cx)
                .unwrap();
        });
        cx.run_until_parked();
        assert_eq!(
            view.read_with(cx, |view, _| view.layout.as_ref().unwrap().rows[0].x),
            left
        );
    }
}
