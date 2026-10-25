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

/// What View > Summary counts in a text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextCounts {
    /// Characters, line breaks left out (CRLF is one line break).
    pub characters: usize,
    /// Runs of letters, digits and `_`.
    pub words: usize,
    pub lines: usize,
    pub bytes: usize,
}

/// Counts characters, words and lines of `text`.
pub fn count_text(text: &Rope) -> TextCounts {
    let mut counts = TextCounts {
        lines: line_count(text),
        bytes: text.len(),
        ..TextCounts::default()
    };
    let mut in_word = false;
    for c in text.chars() {
        if c != '\n' && c != '\r' {
            counts.characters += 1;
        }
        let word = c.is_alphanumeric() || c == '_';
        if word && !in_word {
            counts.words += 1;
        }
        in_word = word;
    }
    counts
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
        _ if ch.is_alphanumeric() || ch == '_' || is_extra_word_char(ch) => CharClass::Word,
        _ if ch.is_whitespace() => CharClass::Space,
        _ => CharClass::Punctuation,
    }
}

/// Characters that are part of words besides letters, digits and `_` (Preferences >
/// Delimiter > Add your character as part of word).
static EXTRA_WORD_CHARS: std::sync::RwLock<Vec<char>> = std::sync::RwLock::new(Vec::new());

/// Makes the characters of `chars` (but spaces) part of words, for word movement, double-click
/// selection, smart highlighting and completion.
pub fn set_extra_word_chars(chars: &str) {
    let chars: Vec<char> = chars.chars().filter(|ch| !ch.is_whitespace()).collect();
    if let Ok(mut extra) = EXTRA_WORD_CHARS.write() {
        *extra = chars;
    }
}

/// Whether `ch` was made part of words by [`set_extra_word_chars`].
pub fn is_extra_word_char(ch: char) -> bool {
    EXTRA_WORD_CHARS
        .read()
        .is_ok_and(|extra| !extra.is_empty() && extra.contains(&ch))
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

/// Go To: a line number (1-based) or a byte offset (0-based, Notepad++'s "position").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoToTarget {
    Line(usize),
    Offset(usize),
}

/// Where Go To puts the caret; out-of-range values go to the last line or the end.
pub fn go_to_position(text: &Rope, target: GoToTarget) -> usize {
    match target {
        GoToTarget::Line(line) => {
            let line = line.clamp(1, line_count(text)) - 1;
            line_range(text, line).start
        }
        GoToTarget::Offset(offset) => {
            let offset = text.floor_char_boundary(offset.min(text.len()));
            // Not between the CR and LF of a line break.
            if offset > 0 && text.byte(offset - 1) == b'\r' && text.get_byte(offset) == Some(b'\n')
            {
                offset - 1
            } else {
                offset
            }
        }
    }
}
