//! Character cells: how many columns of a monospace grid a character takes, and what is drawn in
//! place of characters that have no glyph of their own (tabs, control characters).

use std::ops::Range;

use birchpad_core::Rope;
use unicode_width::UnicodeWidthChar;

/// Width of `ch` in cells, for any character but a tab: 2 for East Asian wide characters and
/// emoji, 0 for combining marks, 1 otherwise (including control characters, drawn as a picture).
pub fn char_cells(ch: char) -> usize {
    if (' '..'\u{7F}').contains(&ch) {
        // Printable ASCII, by far the most common case.
        1
    } else if ch.is_control() {
        1
    } else {
        ch.width().unwrap_or(1)
    }
}

/// Columns a tab advances when it starts at `column`.
pub fn tab_advance(column: usize, tab_width: usize) -> usize {
    let tab_width = tab_width.max(1);
    tab_width - column % tab_width
}

/// Cells taken by `ch` starting at `column` (tabs depend on the column).
pub fn cells_at(ch: char, column: usize, tab_width: usize) -> usize {
    if ch == '\t' {
        tab_advance(column, tab_width)
    } else {
        char_cells(ch)
    }
}

/// What is drawn for a control character: its Unicode control picture (␀, ␛, ...).
pub fn control_picture(ch: char) -> Option<char> {
    match ch {
        '\t' => None,
        '\0'..='\u{1F}' => char::from_u32(0x2400 + ch as u32),
        '\u{7F}' => Some('\u{2421}'),
        '\u{80}'..='\u{9F}' => Some('\u{25AF}'),
        _ => None,
    }
}

/// The column at which `pos` starts, scanning from `start` (at column `start_column`).
pub fn column_after(
    text: &Rope,
    start: usize,
    start_column: usize,
    pos: usize,
    tab_width: usize,
) -> usize {
    let mut column = start_column;
    for ch in text.slice(start..pos).chars() {
        column += cells_at(ch, column, tab_width);
    }
    column
}

/// The character boundary in `range` closest to `target` columns, scanning from `range.start`
/// at `start_column`. A position in the right half of a character rounds up.
pub fn pos_at_column(
    text: &Rope,
    range: Range<usize>,
    start_column: usize,
    target: usize,
    tab_width: usize,
) -> (usize, usize) {
    let mut column = start_column;
    let mut pos = range.start;
    for ch in text.slice(range.clone()).chars() {
        if column >= target {
            break;
        }
        let cells = cells_at(ch, column, tab_width);
        // Past the middle of the character: the caret goes after it.
        if column + cells > target && (target - column) * 2 < cells {
            break;
        }
        column += cells;
        pos += ch.len_utf8();
    }
    (pos, column)
}

/// Text to draw for a range of the document, with tabs expanded to spaces and control
/// characters replaced by pictures, and the mapping between document and display offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayText {
    pub text: String,
    doc_start: usize,
    /// Substituted characters, in order: document range and display range.
    substitutions: Vec<(Range<usize>, Range<usize>)>,
}

impl DisplayText {
    /// Builds the display text of `range`, which starts at column `start_column` of its line.
    pub fn new(text: &Rope, range: Range<usize>, start_column: usize, tab_width: usize) -> Self {
        let mut out = String::with_capacity(range.len());
        let mut substitutions = Vec::new();
        let mut column = start_column;
        let mut pos = range.start;
        for ch in text.slice(range.clone()).chars() {
            let cells = cells_at(ch, column, tab_width);
            let display_start = out.len();
            if ch == '\t' {
                out.extend(std::iter::repeat_n(' ', cells));
            } else if let Some(picture) = control_picture(ch) {
                out.push(picture);
            } else {
                out.push(ch);
            }
            if out.len() - display_start != ch.len_utf8() || ch == '\t' {
                substitutions.push((pos..pos + ch.len_utf8(), display_start..out.len()));
            }
            column += cells;
            pos += ch.len_utf8();
        }
        Self {
            text: out,
            doc_start: range.start,
            substitutions,
        }
    }

    pub fn doc_start(&self) -> usize {
        self.doc_start
    }

    pub fn doc_end(&self) -> usize {
        self.to_doc(self.text.len())
    }

    /// Display offset of a document position inside the range.
    pub fn to_display(&self, pos: usize) -> usize {
        let mut delta: isize = 0;
        for (doc, display) in &self.substitutions {
            if pos <= doc.start {
                break;
            }
            if pos < doc.end {
                return display.start;
            }
            delta += display.len() as isize - doc.len() as isize;
        }
        ((pos - self.doc_start) as isize + delta) as usize
    }

    /// Document position of a display offset; offsets inside an expanded tab or a picture snap
    /// to the nearer end of the character.
    pub fn to_doc(&self, offset: usize) -> usize {
        let mut delta: isize = 0;
        for (doc, display) in &self.substitutions {
            if offset < display.start {
                break;
            }
            if offset < display.end {
                let into = offset - display.start;
                return if into * 2 < display.len() {
                    doc.start
                } else {
                    doc.end
                };
            }
            delta += display.len() as isize - doc.len() as isize;
        }
        (self.doc_start as isize + offset as isize - delta) as usize
    }

    /// The characters of the range in order: document position, character and display range.
    /// One pass, unlike calling [`Self::to_display`] for each.
    pub fn chars<'a>(
        &'a self,
        text: &'a Rope,
    ) -> impl Iterator<Item = (usize, char, Range<usize>)> + 'a {
        let mut substitutions = self.substitutions.iter().peekable();
        let mut pos = self.doc_start;
        let mut offset = 0;
        text.slice(self.doc_start..self.doc_end())
            .chars()
            .map(move |ch| {
                let start = pos;
                pos += ch.len_utf8();
                let display = match substitutions.next_if(|(doc, _)| doc.start == start) {
                    Some((_, display)) => display.clone(),
                    None => offset..offset + ch.len_utf8(),
                };
                offset = display.end;
                (start, ch, display)
            })
    }
}

#[cfg(test)]
mod tests {
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
}
