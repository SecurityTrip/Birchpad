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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_encodings_are_told_from_legacy_ones() {
        for encoding in [Encoding::Utf8, Encoding::Utf16Le, Encoding::Utf16Be] {
            assert!(encoding.is_unicode(), "{encoding}");
        }
        for name in ["windows-1252", "Shift_JIS", ""] {
            assert!(!Encoding::Legacy(name).is_unicode(), "{name:?}");
        }
    }

    #[test]
    fn encodings_show_their_names() {
        assert_eq!(Encoding::Utf8.to_string(), "UTF-8");
        assert_eq!(Encoding::Utf16Le.to_string(), "UTF-16 LE");
        assert_eq!(Encoding::Utf16Be.to_string(), "UTF-16 BE");
        assert_eq!(Encoding::Legacy("KOI8-R").to_string(), "KOI8-R");
        // A legacy encoding is shown by its name as it is, even an empty one.
        assert_eq!(Encoding::Legacy("").to_string(), "");
    }

    #[test]
    fn a_new_document_is_utf8_without_bom_with_the_native_line_ending() {
        let format = Format::default();
        assert_eq!(format, Format::new());
        assert_eq!(format.encoding, Encoding::Utf8);
        assert!(!format.bom);
        assert_eq!(format.line_ending, LineEnding::native());
    }

    #[test]
    fn with_line_ending_changes_only_the_line_ending() {
        let utf16 = Format {
            encoding: Encoding::Utf16Be,
            bom: true,
            line_ending: LineEnding::Lf,
        };
        for line_ending in [LineEnding::CrLf, LineEnding::Lf, LineEnding::Cr] {
            let changed = utf16.with_line_ending(line_ending);
            assert_eq!(changed.line_ending, line_ending);
            assert_eq!((changed.encoding, changed.bom), (Encoding::Utf16Be, true));
        }
    }
}
