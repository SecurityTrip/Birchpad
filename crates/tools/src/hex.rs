//! Hex dumps, as Notepad++'s Hex Editor plugin shows a file: rows of 16 bytes with their offset
//! and the bytes as text, and back from such rows (edited) to bytes.
//!
//! ```text
//! 00000000  48 65 6C 6C 6F 2C 20 77  6F 72 6C 64 21 0A       |Hello, world!.|
//! ```

/// Bytes a row of the dump shows.
pub const BYTES_PER_ROW: usize = 16;

/// Width of a row's hexadecimal part: three columns a byte and one more between its halves.
const HEX_WIDTH: usize = BYTES_PER_ROW * 3 + 1;

/// A dump of `bytes`, a row a line, each line ending with `line_ending`. Empty input dumps to
/// an empty text.
pub fn dump(bytes: &[u8], line_ending: &str) -> String {
    let rows = bytes.len().div_ceil(BYTES_PER_ROW);
    let mut out = String::with_capacity(rows * (10 + HEX_WIDTH + BYTES_PER_ROW + 4));
    for (row, chunk) in bytes.chunks(BYTES_PER_ROW).enumerate() {
        out.push_str(&format!("{:08X}  ", row * BYTES_PER_ROW));
        let mut hex = String::with_capacity(HEX_WIDTH);
        for (index, byte) in chunk.iter().enumerate() {
            if index == BYTES_PER_ROW / 2 {
                hex.push(' ');
            }
            hex.push_str(&format!("{byte:02X} "));
        }
        out.push_str(&format!("{hex:HEX_WIDTH$}|"));
        out.extend(chunk.iter().map(|&byte| {
            if byte.is_ascii_graphic() || byte == b' ' {
                char::from(byte)
            } else {
                '.'
            }
        }));
        out.push('|');
        out.push_str(line_ending);
    }
    out
}

/// A row of a dump that does not read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("line {line}: {message}")]
pub struct HexError {
    /// 1-based.
    pub line: usize,
    pub message: String,
}

/// The bytes of a dump: on each line, the bytes written in hexadecimal between the offset and
/// the `|` that starts the text column. Bytes may be added to a row or taken out of it; the
/// offsets are not checked, and empty lines are skipped.
pub fn parse(text: &str) -> Result<Vec<u8>, HexError> {
    let mut bytes = Vec::with_capacity(text.len() / 4);
    for (index, line) in text.lines().enumerate() {
        let error = |message: String| HexError {
            line: index + 1,
            message,
        };
        let hex = line.split_once('|').map_or(line, |(hex, _)| hex);
        let mut fields = hex.split_ascii_whitespace();
        let Some(offset) = fields.next() else {
            continue;
        };
        if offset.is_empty() || !offset.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(error(format!("{offset:?} is not an offset")));
        }
        for field in fields {
            match u8::from_str_radix(field, 16) {
                Ok(byte) if field.len() == 2 && field.bytes().all(|b| b.is_ascii_hexdigit()) => {
                    bytes.push(byte);
                }
                _ => return Err(error(format!("{field:?} is not a byte in hexadecimal"))),
            }
        }
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn dumps_rows_of_sixteen_bytes() {
        let dumped = dump(b"Hello, world!\n", "\n");
        assert_eq!(
            dumped,
            "00000000  48 65 6C 6C 6F 2C 20 77  6F 72 6C 64 21 0A       |Hello, world!.|\n"
        );
        let two_rows = dump(&[0xFF; 17], "\r\n");
        let lines: Vec<&str> = two_rows.split("\r\n").collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("00000000  FF FF"));
        assert!(lines[0].ends_with("|................|"));
        assert_eq!(lines[1].len(), lines[0].len() - 15);
        assert!(lines[1].starts_with("00000010  FF "));
        assert_eq!(lines[2], "");
        // Every row's text column starts in the same place.
        assert_eq!(lines[0].find('|'), lines[1].find('|'));
    }

    #[test]
    fn empty_dumps_to_nothing() {
        assert_eq!(dump(&[], "\n"), "");
        assert_eq!(parse("").unwrap(), b"");
        assert_eq!(parse("\n\n   \n").unwrap(), b"");
    }

    #[test]
    fn reads_edited_rows() {
        let text = "00000000  41 42 |AB|\n\n00000002 43 44 45\n0000000a  0a|x|y|\n";
        assert_eq!(parse(text).unwrap(), b"ABCDE\n");
        // A row emptied of its bytes.
        assert_eq!(parse("00000000  |ab|\n00000002 41").unwrap(), b"A");
    }

    #[test]
    fn says_which_line_does_not_read() {
        for (text, line) in [
            ("zz 41", 1),
            ("00 4", 1),
            ("00 414", 1),
            ("00 41\n10 G1", 2),
            ("00 +1", 1),
        ] {
            let error = parse(text).expect_err(text);
            assert_eq!(error.line, line, "{text}");
        }
        assert_eq!(
            parse("00 4").unwrap_err().to_string(),
            "line 1: \"4\" is not a byte in hexadecimal"
        );
    }

    proptest! {
        #[test]
        fn reads_back_what_it_dumps(bytes in proptest::collection::vec(any::<u8>(), 0..100)) {
            prop_assert_eq!(parse(&dump(&bytes, "\n")).unwrap(), bytes.clone());
            prop_assert_eq!(parse(&dump(&bytes, "\r\n")).unwrap(), bytes);
        }
    }
}
