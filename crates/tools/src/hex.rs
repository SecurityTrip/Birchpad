//! Hex dumps, as Notepad++'s Hex Editor plugin shows a file: rows of 16 bytes with their offset
//! and the bytes as text, and back from such rows (edited) to bytes.
//!
//! ```text
//! 00000000  48 65 6C 6C 6F 2C 20 77  6F 72 6C 64 21 0A       |Hello, world!.|
//! ```

use birchpad_core::{Edit, InvalidEdit, Rope, Selection, Transaction};

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
#[error("{}", birchpad_i18n::t!("tools.hex.line", line = .line, message = .message))]
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
            return Err(error(birchpad_i18n::t!(
                "tools.hex.not-an-offset",
                text = format!("{offset:?}")
            )));
        }
        for field in fields {
            match u8::from_str_radix(field, 16) {
                Ok(byte) if field.len() == 2 && field.bytes().all(|b| b.is_ascii_hexdigit()) => {
                    bytes.push(byte);
                }
                _ => {
                    return Err(error(birchpad_i18n::t!(
                        "tools.hex.not-a-byte",
                        text = format!("{field:?}")
                    )));
                }
            }
        }
    }
    Ok(bytes)
}

/// The edit that applies a dump read back to the document it shows: it turns `old` into `new`,
/// touching only what differs between them, and leaves the caret after what was put in.
pub fn replace_differing(old: &Rope, new: &str) -> Result<Transaction, InvalidEdit> {
    let old_text = old.to_string();
    let prefix = old_text
        .char_indices()
        .zip(new.chars())
        .find(|((_, a), b)| a != b)
        .map_or(old_text.len().min(new.len()), |((at, _), _)| at);
    let prefix = (0..=prefix)
        .rev()
        .find(|&at| old_text.is_char_boundary(at) && new.is_char_boundary(at))
        .unwrap_or(0);
    let old_rest = &old_text[prefix..];
    let new_rest = &new[prefix..];
    let mut suffix = old_rest
        .bytes()
        .rev()
        .zip(new_rest.bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    while !old_rest.is_char_boundary(old_rest.len() - suffix)
        || !new_rest.is_char_boundary(new_rest.len() - suffix)
    {
        suffix -= 1;
    }
    let range = prefix..old_text.len() - suffix;
    let inserted = &new_rest[..new_rest.len() - suffix];
    Ok(
        Transaction::from_edits(old, [Edit::replace(range.clone(), inserted)])?
            .with_selection(Selection::point(range.start + inserted.len())),
    )
}

#[cfg(test)]
#[path = "../tests/unit/hex/tests.rs"]
mod tests;
