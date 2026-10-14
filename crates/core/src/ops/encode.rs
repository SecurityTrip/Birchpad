//! Converting the selected text, as Notepad++'s MIME Tools and Converter plugins do: Base64,
//! URL (percent) encoding, and ASCII to hexadecimal and back. Each works on the UTF-8 bytes of
//! the text; decoding refuses what does not decode to valid UTF-8.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ropey::Rope;

use crate::change::Edit;
use crate::selection::{Range as Span, Selection};
use crate::transaction::Transaction;

/// A conversion of the selected text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conversion {
    Base64Encode,
    Base64Decode,
    UrlEncode,
    UrlDecode,
    /// `Hi` to `4869`.
    AsciiToHex,
    /// `48 69` or `4869` to `Hi`.
    HexToAscii,
}

/// Why the selected text could not be converted.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EncodeError {
    #[error("{}", birchpad_i18n::t!("core.encode.not-base64"))]
    Base64,
    #[error("{}", birchpad_i18n::t!("core.encode.not-url-encoding", reason = .0))]
    Url(String),
    #[error("{}", birchpad_i18n::t!("core.encode.not-hexadecimal", reason = .0))]
    Hex(String),
    #[error("{}", birchpad_i18n::t!("core.encode.not-utf8"))]
    NotText,
}

/// Converts `text` with `encoding`.
pub fn encode_text(text: &str, conversion: Conversion) -> Result<String, EncodeError> {
    let text_of = |bytes: Vec<u8>| String::from_utf8(bytes).map_err(|_| EncodeError::NotText);
    match conversion {
        Conversion::Base64Encode => Ok(STANDARD.encode(text)),
        Conversion::Base64Decode => {
            let compact: String = text.chars().filter(|ch| !ch.is_whitespace()).collect();
            text_of(STANDARD.decode(compact).map_err(|_| EncodeError::Base64)?)
        }
        Conversion::UrlEncode => Ok(url_encode(text)),
        Conversion::UrlDecode => text_of(url_decode(text)?),
        Conversion::AsciiToHex => Ok(text.bytes().map(|byte| format!("{byte:02X}")).collect()),
        Conversion::HexToAscii => text_of(hex_decode(text)?),
    }
}

/// Percent-encodes everything but RFC 3986's unreserved characters.
fn url_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn hex_value(byte: u8) -> Option<u8> {
    (byte as char).to_digit(16).map(|digit| digit as u8)
}

/// Decodes `%XX`; `+` stays a plus, as in a URL's path.
fn url_decode(text: &str) -> Result<Vec<u8>, EncodeError> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let pair = bytes.get(index + 1..index + 3);
            let value = pair.and_then(|pair| Some(hex_value(pair[0])? << 4 | hex_value(pair[1])?));
            let value = value.ok_or_else(|| {
                let end = text.ceil_char_boundary((index + 3).min(text.len()));
                EncodeError::Url(format!("`{}` is not %XX", &text[index..end]))
            })?;
            out.push(value);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    Ok(out)
}

/// Pairs of hexadecimal digits, spaces and line breaks between pairs ignored.
fn hex_decode(text: &str) -> Result<Vec<u8>, EncodeError> {
    let digits: Vec<u8> = text
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    if let Some(bad) = text
        .chars()
        .find(|ch| !ch.is_whitespace() && !ch.is_ascii_hexdigit())
    {
        return Err(EncodeError::Hex(format!(
            "`{bad}` is not a hexadecimal digit"
        )));
    }
    if !digits.len().is_multiple_of(2) {
        return Err(EncodeError::Hex("an odd number of digits".into()));
    }
    Ok(digits
        .chunks(2)
        .map(|pair| hex_value(pair[0]).unwrap_or(0) << 4 | hex_value(pair[1]).unwrap_or(0))
        .collect())
}

/// Converts every non-empty selection range, which then selects the result; one undo step.
/// `Ok(None)` when nothing is selected or nothing changes; the first range that does not
/// convert fails it all.
pub fn encode_selections(
    text: &Rope,
    selection: &Selection,
    conversion: Conversion,
) -> Result<Option<Transaction>, EncodeError> {
    let mut edits = Vec::new();
    let mut ranges = Vec::new();
    let mut shift: isize = 0;
    for range in selection.iter() {
        let start = (range.from() as isize + shift) as usize;
        if range.is_empty() {
            ranges.push(Span::point(start));
            continue;
        }
        let original = text.slice(range.from()..range.to()).to_string();
        let converted = encode_text(&original, conversion)?;
        let end = start + converted.len();
        ranges.push(if range.is_backward() {
            Span::new(end, start)
        } else {
            Span::new(start, end)
        });
        shift += converted.len() as isize - original.len() as isize;
        if converted != original {
            edits.push(Edit::replace(range.from()..range.to(), converted));
        }
    }
    if edits.is_empty() {
        return Ok(None);
    }
    let Ok(transaction) = Transaction::from_edits(text, edits) else {
        return Ok(None);
    };
    Ok(Some(transaction.with_selection(Selection::new(
        ranges,
        selection.primary_index(),
    ))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn base64() {
        assert_eq!(
            encode_text("Hello, мир!", Conversion::Base64Encode).unwrap(),
            "SGVsbG8sINC80LjRgCE="
        );
        assert_eq!(
            encode_text("SGVsbG8sINC80LjRgCE=", Conversion::Base64Decode).unwrap(),
            "Hello, мир!"
        );
        // Line breaks inside are fine, as mail wraps Base64.
        assert_eq!(
            encode_text("SGVs\r\nbG8=", Conversion::Base64Decode).unwrap(),
            "Hello"
        );
        assert_eq!(encode_text("", Conversion::Base64Encode).unwrap(), "");
        assert_eq!(encode_text("", Conversion::Base64Decode).unwrap(), "");
        for bad in ["SGVsbG8", "S", "****", "SGVsbG8=="] {
            assert_eq!(
                encode_text(bad, Conversion::Base64Decode),
                Err(EncodeError::Base64),
                "{bad}"
            );
        }
        // Bytes that are not UTF-8.
        assert_eq!(
            encode_text("/w==", Conversion::Base64Decode),
            Err(EncodeError::NotText)
        );
    }

    #[test]
    fn url() {
        assert_eq!(
            encode_text("a b&c=д/~", Conversion::UrlEncode).unwrap(),
            "a%20b%26c%3D%D0%B4%2F~"
        );
        assert_eq!(
            encode_text("a%20b%26c%3d%D0%B4+", Conversion::UrlDecode).unwrap(),
            "a b&c=д+"
        );
        assert_eq!(encode_text("", Conversion::UrlDecode).unwrap(), "");
        for bad in ["%", "%2", "%zz", "a%2g", "%д"] {
            assert!(
                matches!(
                    encode_text(bad, Conversion::UrlDecode),
                    Err(EncodeError::Url(_))
                ),
                "{bad}"
            );
        }
        assert_eq!(
            encode_text("%FF", Conversion::UrlDecode),
            Err(EncodeError::NotText)
        );
    }

    #[test]
    fn hex() {
        assert_eq!(
            encode_text("Hi\n", Conversion::AsciiToHex).unwrap(),
            "48690A"
        );
        assert_eq!(encode_text("д", Conversion::AsciiToHex).unwrap(), "D0B4");
        assert_eq!(
            encode_text("48 69\n0a", Conversion::HexToAscii).unwrap(),
            "Hi\n"
        );
        assert_eq!(encode_text("", Conversion::HexToAscii).unwrap(), "");
        assert!(matches!(
            encode_text("486", Conversion::HexToAscii),
            Err(EncodeError::Hex(_))
        ));
        assert!(matches!(
            encode_text("4G", Conversion::HexToAscii),
            Err(EncodeError::Hex(_))
        ));
        assert!(matches!(
            encode_text("0x48", Conversion::HexToAscii),
            Err(EncodeError::Hex(_))
        ));
        assert_eq!(
            encode_text("FF", Conversion::HexToAscii),
            Err(EncodeError::NotText)
        );
    }

    #[test]
    fn selections_convert_in_one_step() {
        let text = Rope::from_str("ab cd ef");
        let selection = Selection::new(vec![Span::new(0, 2), Span::point(3), Span::new(8, 6)], 0);
        let transaction = encode_selections(&text, &selection, Conversion::AsciiToHex)
            .unwrap()
            .unwrap();
        let mut result = text.clone();
        transaction.changes().apply(&mut result);
        assert_eq!(result.to_string(), "6162 cd 6566");
        let after = transaction.selection().unwrap();
        let spans: Vec<_> = after
            .iter()
            .map(|range| (range.anchor, range.head))
            .collect();
        assert_eq!(spans, [(0, 4), (5, 5), (12, 8)]);
        // Nothing selected, and a range that does not convert.
        assert_eq!(
            encode_selections(&text, &Selection::point(3), Conversion::AsciiToHex),
            Ok(None)
        );
        let bad = Selection::new(vec![Span::new(0, 2), Span::new(3, 5)], 0);
        assert!(encode_selections(&text, &bad, Conversion::HexToAscii).is_err());
    }

    proptest! {
        #[test]
        fn encodings_round_trip(text in "\\PC*") {
            for (to, back) in [
                (Conversion::Base64Encode, Conversion::Base64Decode),
                (Conversion::UrlEncode, Conversion::UrlDecode),
                (Conversion::AsciiToHex, Conversion::HexToAscii),
            ] {
                let encoded = encode_text(&text, to).unwrap();
                prop_assert!(encoded.is_ascii());
                prop_assert_eq!(encode_text(&encoded, back).unwrap(), text.clone());
            }
        }
    }
}
