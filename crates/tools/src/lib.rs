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
pub mod print;
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
