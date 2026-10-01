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

/// How characters group into words for word movement and double-click selection, as in
/// Scintilla: letters, digits and `_` form words; other symbols form punctuation runs; spaces
/// and line breaks separate them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharClass {
    Space,
    LineBreak,
    Word,
    Punctuation,
}

pub fn char_class(ch: char) -> CharClass {
    match ch {
        ' ' | '\t' => CharClass::Space,
        '\r' | '\n' => CharClass::LineBreak,
        _ if ch.is_alphanumeric() || ch == '_' => CharClass::Word,
        _ if ch.is_whitespace() => CharClass::Space,
        _ => CharClass::Punctuation,
    }
}

/// Ctrl+Right: to the start of the next word, past any spaces. A line break is a stop of its
/// own.
pub fn word_right(text: &Rope, pos: usize) -> usize {
    let mut chars = text.chars_at(pos);
    let Some(first) = chars.next() else {
        return pos;
    };
    let mut pos = pos;
    let class = char_class(first);
    if class == CharClass::LineBreak {
        return next_boundary(text, pos);
    }
    pos += first.len_utf8();
    let mut skipping = class;
    for ch in chars {
        let next = char_class(ch);
        if next == CharClass::LineBreak {
            break;
        }
        if next != skipping {
            if next != CharClass::Space {
                break;
            }
            skipping = CharClass::Space;
        }
        pos += ch.len_utf8();
    }
    pos
}

/// Ctrl+Left: back past any spaces to the start of the previous word. A line break is a stop
/// of its own.
pub fn word_left(text: &Rope, pos: usize) -> usize {
    let mut chars = text.chars_at(pos);
    let mut pos = pos;
    let mut skipping: Option<CharClass> = None;
    while let Some(ch) = chars.prev() {
        let class = char_class(ch);
        match skipping {
            None if class == CharClass::LineBreak => return prev_boundary(text, pos),
            None => skipping = Some(class),
            Some(CharClass::Space) if class != CharClass::Space => {
                if class == CharClass::LineBreak {
                    break;
                }
                skipping = Some(class);
            }
            Some(current) if current != class => break,
            Some(_) => {}
        }
        pos -= ch.len_utf8();
    }
    pos
}

/// The run of same-class characters at `pos` (a word, punctuation or spaces), for double-click
/// selection. At the end of a word, the word before the caret.
pub fn word_at(text: &Rope, pos: usize) -> Range<usize> {
    let after = text.chars_at(pos).next().map(char_class);
    let before = text.chars_at(pos).prev().map(char_class);
    let class = match (before, after) {
        (Some(CharClass::Word), Some(class)) if class != CharClass::Word => CharClass::Word,
        (_, Some(class)) if class != CharClass::LineBreak => class,
        (Some(class), _) if class != CharClass::LineBreak => class,
        _ => return pos..pos,
    };
    let mut start = pos;
    let mut backward = text.chars_at(pos);
    while let Some(ch) = backward.prev() {
        if char_class(ch) != class {
            break;
        }
        start -= ch.len_utf8();
    }
    let mut end = pos;
    for ch in text.chars_at(pos) {
        if char_class(ch) != class {
            break;
        }
        end += ch.len_utf8();
    }
    start..end
}

/// The first character of `line` that is not a space or tab (the line end if there is none).
pub fn indent_end(text: &Rope, line: usize) -> usize {
    let range = line_range(text, line);
    let indent: usize = text
        .slice(range.clone())
        .chars()
        .take_while(|&ch| ch == ' ' || ch == '\t')
        .map(char::len_utf8)
        .sum();
    range.start + indent
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
