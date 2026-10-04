//! Line endings. Like Notepad++, a document has one line-ending mode used for new lines, while
//! the text itself may contain any mix of CRLF, LF and CR.

use ropey::Rope;

use crate::change::Edit;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LineEnding {
    /// `\r\n`, Windows.
    CrLf,
    /// `\n`, Unix and macOS.
    Lf,
    /// `\r`, classic Mac OS.
    Cr,
}

impl LineEnding {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CrLf => "\r\n",
            Self::Lf => "\n",
            Self::Cr => "\r",
        }
    }

    /// The platform's conventional line ending.
    pub const fn native() -> Self {
        if cfg!(windows) { Self::CrLf } else { Self::Lf }
    }

    /// Detects the line ending of a text from its first line break, as Notepad++ does.
    /// Returns `None` if the text has no line breaks.
    pub fn detect(text: &Rope) -> Option<Self> {
        let mut bytes = text.bytes();
        while let Some(byte) = bytes.next() {
            match byte {
                b'\n' => return Some(Self::Lf),
                b'\r' => {
                    return Some(if bytes.next() == Some(b'\n') {
                        Self::CrLf
                    } else {
                        Self::Cr
                    });
                }
                _ => {}
            }
        }
        None
    }

    /// The line break that starts at byte `pos`, if any (what View > Show End of Line labels).
    pub fn at(text: &Rope, pos: usize) -> Option<Self> {
        match text.get_byte(pos)? {
            b'\n' => Some(Self::Lf),
            b'\r' if text.get_byte(pos + 1) == Some(b'\n') => Some(Self::CrLf),
            b'\r' => Some(Self::Cr),
            _ => None,
        }
    }

    /// How View > Show End of Line labels the line break.
    pub const fn label(self) -> &'static str {
        match self {
            Self::CrLf => "CRLF",
            Self::Lf => "LF",
            Self::Cr => "CR",
        }
    }
}

/// Edits that turn every line break of `text` (CRLF, LF or CR) into `target`.
pub fn convert_line_breaks(text: &Rope, target: LineEnding) -> Vec<Edit> {
    let mut edits = Vec::new();
    let mut pos = 0;
    // A CR at the end of the previous chunk, waiting to see whether an LF follows.
    let mut pending_cr: Option<usize> = None;
    for chunk in text.chunks() {
        for (i, &byte) in chunk.as_bytes().iter().enumerate() {
            let at = pos + i;
            match (pending_cr.take(), byte) {
                (Some(cr), b'\n') => push_break(&mut edits, cr..at + 1, LineEnding::CrLf, target),
                (Some(cr), _) => {
                    push_break(&mut edits, cr..cr + 1, LineEnding::Cr, target);
                    if byte == b'\r' {
                        pending_cr = Some(at);
                    }
                }
                (None, b'\r') => pending_cr = Some(at),
                (None, b'\n') => push_break(&mut edits, at..at + 1, LineEnding::Lf, target),
                (None, _) => {}
            }
        }
        pos += chunk.len();
    }
    if let Some(cr) = pending_cr {
        push_break(&mut edits, cr..cr + 1, LineEnding::Cr, target);
    }
    edits
}

fn push_break(
    edits: &mut Vec<Edit>,
    range: std::ops::Range<usize>,
    found: LineEnding,
    target: LineEnding,
) {
    if found != target {
        edits.push(Edit::replace(range, target.as_str()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change::ChangeSet;

    fn convert(text: &str, target: LineEnding) -> String {
        let mut rope = Rope::from_str(text);
        let changes = ChangeSet::from_edits(&rope, convert_line_breaks(&rope, target)).unwrap();
        changes.apply(&mut rope);
        rope.to_string()
    }

    #[test]
    fn converts_every_kind_of_break() {
        let mixed = "a\r\nb\nc\rd\r\r\ne\r";
        assert_eq!(convert(mixed, LineEnding::Lf), "a\nb\nc\nd\n\ne\n");
        assert_eq!(
            convert(mixed, LineEnding::CrLf),
            "a\r\nb\r\nc\r\nd\r\n\r\ne\r\n"
        );
        assert_eq!(convert(mixed, LineEnding::Cr), "a\rb\rc\rd\r\re\r");
        assert!(convert_line_breaks(&Rope::from_str("x\ny\n"), LineEnding::Lf).is_empty());
    }

    #[test]
    fn line_break_at_a_position() {
        let text = Rope::from_str("a\r\nb\nc\rd");
        assert_eq!(LineEnding::at(&text, 0), None);
        assert_eq!(LineEnding::at(&text, 1), Some(LineEnding::CrLf));
        assert_eq!(LineEnding::at(&text, 4), Some(LineEnding::Lf));
        assert_eq!(LineEnding::at(&text, 6), Some(LineEnding::Cr));
        assert_eq!(LineEnding::at(&text, 8), None, "end of text");
        assert_eq!(LineEnding::CrLf.label(), "CRLF");
    }

    #[test]
    fn crlf_split_across_chunks_is_one_break() {
        // Long enough for ropey to split it into several chunks at arbitrary places.
        let text = "abc\r\n".repeat(5000);
        let converted = convert(&text, LineEnding::Lf);
        assert_eq!(converted, "abc\n".repeat(5000));
    }

    #[test]
    fn detects_first_line_break() {
        let detect = |s: &str| LineEnding::detect(&Rope::from_str(s));
        assert_eq!(detect("a\r\nb\nc"), Some(LineEnding::CrLf));
        assert_eq!(detect("a\nb\r\nc"), Some(LineEnding::Lf));
        assert_eq!(detect("a\rb"), Some(LineEnding::Cr));
        assert_eq!(detect("trailing\r"), Some(LineEnding::Cr));
        assert_eq!(detect("one line"), None);
    }
}
