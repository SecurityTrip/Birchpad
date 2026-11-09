//! Byte offsets into a document and the protocol's positions: a line (CRLF, LF and CR end
//! lines, as in Birchpad) and a character within it, counted in UTF-8 bytes or in UTF-16 code
//! units as agreed with the server (ADR 0065).

use std::ops::Range;

use birchpad_core::Rope;
use birchpad_core::motion::{line_count, line_of, line_range};
use lsp_types::{Position, PositionEncodingKind};

/// How the characters of a position are counted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PositionEncoding {
    Utf8,
    /// The protocol's default.
    #[default]
    Utf16,
}

impl PositionEncoding {
    /// The encoding a server chose; UTF-16 when it says nothing or names one not offered.
    pub fn of_server(kind: Option<&PositionEncodingKind>) -> Self {
        match kind.map(PositionEncodingKind::as_str) {
            Some("utf-8") => Self::Utf8,
            _ => Self::Utf16,
        }
    }

    /// The encodings offered to servers, the preferred first.
    pub fn offered() -> Vec<PositionEncodingKind> {
        vec![PositionEncodingKind::UTF8, PositionEncodingKind::UTF16]
    }
}

/// The position of byte `offset` (past the end: the end of the text).
pub fn position(text: &Rope, offset: usize, encoding: PositionEncoding) -> Position {
    let offset = text.floor_char_boundary(offset.min(text.len()));
    let line = line_of(text, offset);
    let start = line_range(text, line).start;
    let before = text.slice(start..offset);
    let character = match encoding {
        PositionEncoding::Utf8 => before.len(),
        PositionEncoding::Utf16 => before.chars().map(char::len_utf16).sum(),
    };
    Position::new(line as u32, character as u32)
}

/// The byte offset of `position`: past the end of its line, the end of the line; inside a
/// character, its start; past the last line, the end of the text.
pub fn offset(text: &Rope, position: Position, encoding: PositionEncoding) -> usize {
    let line = position.line as usize;
    if line >= line_count(text) {
        return text.len();
    }
    let bounds = line_range(text, line);
    let wanted = position.character as usize;
    let mut counted = 0;
    let mut at = bounds.start;
    for c in text.slice(bounds.clone()).chars() {
        let width = match encoding {
            PositionEncoding::Utf8 => c.len_utf8(),
            PositionEncoding::Utf16 => c.len_utf16(),
        };
        if counted + width > wanted {
            return at;
        }
        counted += width;
        at += c.len_utf8();
    }
    bounds.end
}

/// The range of bytes `range`.
pub fn lsp_range(text: &Rope, range: Range<usize>, encoding: PositionEncoding) -> lsp_types::Range {
    lsp_types::Range::new(
        position(text, range.start, encoding),
        position(text, range.end, encoding),
    )
}

/// The bytes of `range`; one that ends before it starts is empty, at its start.
pub fn byte_range(
    text: &Rope,
    range: lsp_types::Range,
    encoding: PositionEncoding,
) -> Range<usize> {
    let start = offset(text, range.start, encoding);
    let end = offset(text, range.end, encoding);
    start..end.max(start)
}
