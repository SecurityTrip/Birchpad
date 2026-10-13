//! Birchpad's built-in tools, without a window: what Notepad++ gets from its most popular
//! plugins (JSON Viewer, XML Tools, Hex Editor, Compare, DSpellCheck), as pure functions over
//! text and bytes.
//!
//! Positions are byte offsets into the text, as everywhere in Birchpad.

pub mod compare;
pub mod date_time;
pub mod hex;
pub mod html;
pub mod json;
pub mod spell;
pub mod xml;

use std::fmt;

/// Text that does not parse, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    /// Byte offset of the problem in the text.
    pub offset: usize,
    pub message: String,
}

impl SyntaxError {
    pub(crate) fn new(offset: usize, message: impl Into<String>) -> Self {
        Self {
            offset,
            message: message.into(),
        }
    }

    /// The 1-based line and column (in characters) of the problem in `text`.
    pub fn line_column(&self, text: &str) -> (usize, usize) {
        let offset = floor_char_boundary(text, self.offset);
        let before = &text[..offset];
        let line = before.matches('\n').count() + 1;
        let line_start = before.rfind('\n').map_or(0, |at| at + 1);
        (line, before[line_start..].chars().count() + 1)
    }
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SyntaxError {}

/// One level of indentation in formatted output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indent {
    Tab,
    /// That many spaces (0 puts every level at the start of the line).
    Spaces(u8),
}

impl Indent {
    pub(crate) fn write(self, out: &mut String, depth: usize) {
        for _ in 0..depth {
            match self {
                Self::Tab => out.push('\t'),
                Self::Spaces(count) => out.extend(std::iter::repeat_n(' ', count.into())),
            }
        }
    }
}

/// How formatted output is laid out: the indentation and the line break.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout<'a> {
    pub indent: Indent,
    pub line_ending: &'a str,
}

impl Default for Layout<'_> {
    fn default() -> Self {
        Self {
            indent: Indent::Spaces(4),
            line_ending: "\n",
        }
    }
}

pub(crate) fn floor_char_boundary(text: &str, mut offset: usize) -> usize {
    offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_and_column_count_characters() {
        let text = "ab\nмир x";
        assert_eq!(SyntaxError::new(0, "").line_column(text), (1, 1));
        assert_eq!(SyntaxError::new(2, "").line_column(text), (1, 3));
        assert_eq!(SyntaxError::new(3, "").line_column(text), (2, 1));
        // After three two-byte letters and a space.
        assert_eq!(SyntaxError::new(10, "").line_column(text), (2, 5));
        // Inside a character, and past the end.
        assert_eq!(SyntaxError::new(4, "").line_column(text), (2, 1));
        assert_eq!(SyntaxError::new(99, "").line_column(text), (2, 6));
        assert_eq!(SyntaxError::new(0, "").line_column(""), (1, 1));
    }

    #[test]
    fn indents_by_tabs_or_spaces() {
        let mut out = String::new();
        Indent::Tab.write(&mut out, 2);
        Indent::Spaces(3).write(&mut out, 1);
        Indent::Spaces(0).write(&mut out, 5);
        Indent::Spaces(2).write(&mut out, 0);
        assert_eq!(out, "\t\t   ");
        assert_eq!(SyntaxError::new(1, "bad").to_string(), "bad");
    }
}
