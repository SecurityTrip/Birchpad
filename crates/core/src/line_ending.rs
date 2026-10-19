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
    /// NEL, the line break of EBCDIC files on IBM mainframes (byte 0x15). The text keeps it as
    /// `\n`, where the editor breaks lines; saving in an EBCDIC code page writes NEL.
    Nel,
}

impl LineEnding {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CrLf => "\r\n",
            Self::Lf | Self::Nel => "\n",
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
            Self::Nel => "NEL",
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
    // NEL is LF in the text: converting between them changes nothing.
    if found.as_str() != target.as_str() {
        edits.push(Edit::replace(range, target.as_str()));
    }
}

#[cfg(test)]
#[path = "../tests/unit/line_ending/tests.rs"]
mod tests;
