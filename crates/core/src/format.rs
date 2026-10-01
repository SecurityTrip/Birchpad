//! How a document is stored on disk: encoding, byte order mark and line-ending mode.
//!
//! This is a description only; `birchpad-io` converts between bytes and text. The format lives
//! in the document (not in the file layer) because changing it is an edit: "Convert to UTF-8" and
//! "EOL Conversion" are undoable and make the document modified.

use std::fmt;

use crate::line_ending::LineEnding;

/// A text encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Encoding {
    Utf8,
    Utf16Le,
    Utf16Be,
    /// A legacy single- or multi-byte encoding, by its WHATWG name (`windows-1251`, `KOI8-R`,
    /// `Shift_JIS`, ...).
    Legacy(&'static str),
}

impl Encoding {
    pub const fn is_unicode(self) -> bool {
        !matches!(self, Self::Legacy(_))
    }
}

/// Encoding, BOM and line-ending mode of a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Format {
    pub encoding: Encoding,
    /// Whether the file starts with a byte order mark. Only meaningful for Unicode encodings.
    pub bom: bool,
    /// Line ending used for new lines. The text itself may contain any mix of line breaks.
    pub line_ending: LineEnding,
}

impl Format {
    /// UTF-8 without BOM, with the platform's line ending: the format of a new document.
    pub const fn new() -> Self {
        Self {
            encoding: Encoding::Utf8,
            bom: false,
            line_ending: LineEnding::native(),
        }
    }

    pub const fn with_line_ending(mut self, line_ending: LineEnding) -> Self {
        self.line_ending = line_ending;
        self
    }
}

impl Default for Format {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for Encoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Utf8 => f.write_str("UTF-8"),
            Self::Utf16Le => f.write_str("UTF-16 LE"),
            Self::Utf16Be => f.write_str("UTF-16 BE"),
            Self::Legacy(name) => f.write_str(name),
        }
    }
}
