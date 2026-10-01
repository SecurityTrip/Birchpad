//! Line endings. Like Notepad++, a document has one line-ending mode used for new lines, while
//! the text itself may contain any mix of CRLF, LF and CR.

use ropey::Rope;

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
}

#[cfg(test)]
mod tests {
    use super::*;

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
