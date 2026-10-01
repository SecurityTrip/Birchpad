//! Positions in the text that carets move between: character boundaries, line ranges.
//!
//! These are pure functions of the text, so views stay thin and every rule is unit-tested.

use std::ops::Range;

use ropey::{LineType, Rope};

/// Line breaks recognized everywhere in Birchpad: CRLF, LF and CR, as in Scintilla.
pub const LINE_TYPE: LineType = LineType::LF_CR;

/// Number of lines; an empty text and a text ending with a line break both count the last,
/// empty line.
pub fn line_count(text: &Rope) -> usize {
    text.len_lines(LINE_TYPE)
}

/// The line containing byte `pos`.
pub fn line_of(text: &Rope, pos: usize) -> usize {
    text.byte_to_line_idx(pos, LINE_TYPE)
}

/// Byte range of `line` without its line break.
pub fn line_range(text: &Rope, line: usize) -> Range<usize> {
    let start = text.line_to_byte_idx(line, LINE_TYPE);
    let slice = text.line(line, LINE_TYPE);
    start
        ..start
            + slice
                .trailing_line_break_idx(LINE_TYPE)
                .unwrap_or(slice.len())
}

/// Byte range of `line` including its line break.
pub fn line_range_with_break(text: &Rope, line: usize) -> Range<usize> {
    let start = text.line_to_byte_idx(line, LINE_TYPE);
    start..start + text.line(line, LINE_TYPE).len()
}

/// The previous caret position; a CRLF pair is a single step.
pub fn prev_boundary(text: &Rope, pos: usize) -> usize {
    match pos {
        0 => 0,
        _ if pos >= 2 && text.byte(pos - 2) == b'\r' && text.byte(pos - 1) == b'\n' => pos - 2,
        _ => text.floor_char_boundary(pos - 1),
    }
}

/// The next caret position; a CRLF pair is a single step.
pub fn next_boundary(text: &Rope, pos: usize) -> usize {
    if pos >= text.len() {
        text.len()
    } else if text.byte(pos) == b'\r' && text.get_byte(pos + 1) == Some(b'\n') {
        pos + 2
    } else {
        text.ceil_char_boundary(pos + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
