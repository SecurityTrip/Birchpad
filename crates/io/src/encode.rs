//! Text to bytes. Encoding never replaces characters: if the text cannot be represented, the
//! caller gets the list of offending characters and nothing is written.

use std::fmt;

use birchpad_core::{Encoding, Rope};
use encoding_rs::EncoderResult;

use crate::decode::bom_bytes;
use crate::encoding::{display_name, to_encoding_rs};

/// How many offending characters [`Unencodable`] keeps for the error message.
const SAMPLES: usize = 10;

/// Characters that do not exist in the target encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unencodable {
    pub encoding: Encoding,
    /// Total number of characters that cannot be encoded.
    pub count: usize,
    /// The first few of them, with their byte offsets in the text.
    pub samples: Vec<(usize, char)>,
}

impl fmt::Display for Unencodable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let chars: Vec<String> = self
            .samples
            .iter()
            .map(|(_, ch)| format!("'{ch}' (U+{:04X})", u32::from(*ch)))
            .collect();
        write!(
            f,
            "{} character{} cannot be saved in {}: {}{}",
            self.count,
            if self.count == 1 { "" } else { "s" },
            display_name(self.encoding, false),
            chars.join(", "),
            if self.count > self.samples.len() {
                ", ..."
            } else {
                ""
            }
        )
    }
}

impl std::error::Error for Unencodable {}

/// Encodes `text`, with a byte order mark if `bom` is set and the encoding is Unicode.
pub fn encode(text: &Rope, encoding: Encoding, bom: bool) -> Result<Vec<u8>, Unencodable> {
    let mut out = Vec::new();
    if bom {
        out.extend_from_slice(bom_bytes(encoding));
    }
    match encoding {
        Encoding::Utf8 => {
            out.reserve(text.len());
            for chunk in text.chunks() {
                out.extend_from_slice(chunk.as_bytes());
            }
        }
        Encoding::Utf16Le | Encoding::Utf16Be => {
            out.reserve(text.len_utf16() * 2);
            let little = encoding == Encoding::Utf16Le;
            for chunk in text.chunks() {
                for unit in chunk.encode_utf16() {
                    let bytes = if little {
                        unit.to_le_bytes()
                    } else {
                        unit.to_be_bytes()
                    };
                    out.extend_from_slice(&bytes);
                }
            }
        }
        Encoding::Legacy(_) => encode_legacy(text, encoding, &mut out)?,
    }
    Ok(out)
}

/// Checks that `text` can be encoded, without keeping the bytes.
pub fn check_encodable(text: &Rope, encoding: Encoding) -> Result<(), Unencodable> {
    match encoding {
        Encoding::Utf8 | Encoding::Utf16Le | Encoding::Utf16Be => Ok(()),
        Encoding::Legacy(_) => encode(text, encoding, false).map(drop),
    }
}

fn encode_legacy(text: &Rope, encoding: Encoding, out: &mut Vec<u8>) -> Result<(), Unencodable> {
    let mut encoder = to_encoding_rs(encoding).new_encoder();
    let mut problem = Unencodable {
        encoding,
        count: 0,
        samples: Vec::new(),
    };
    let mut base = 0;
    for chunk in text.chunks() {
        let mut pos = 0;
        while pos < chunk.len() {
            let rest = &chunk[pos..];
            let needed = encoder
                .max_buffer_length_from_utf8_without_replacement(rest.len())
                .unwrap_or(rest.len() * 4);
            out.reserve(needed);
            let (result, read) =
                encoder.encode_from_utf8_to_vec_without_replacement(rest, out, false);
            pos += read;
            match result {
                EncoderResult::InputEmpty => break,
                EncoderResult::OutputFull => {}
                EncoderResult::Unmappable(ch) => {
                    problem.count += 1;
                    if problem.samples.len() < SAMPLES {
                        problem.samples.push((base + pos - ch.len_utf8(), ch));
                    }
                }
            }
        }
        base += chunk.len();
    }
    // Flush the final state of stateful encodings (ISO-2022-JP).
    loop {
        out.reserve(16);
        let (result, _) = encoder.encode_from_utf8_to_vec_without_replacement("", out, true);
        if !matches!(result, EncoderResult::OutputFull) {
            break;
        }
    }
    if problem.count > 0 {
        Err(problem)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_boms_and_utf16() {
        let text = Rope::from_str("aж");
        assert_eq!(
            encode(&text, Encoding::Utf8, true).unwrap(),
            b"\xEF\xBB\xBFa\xD0\xB6"
        );
        assert_eq!(
            encode(&text, Encoding::Utf16Le, true).unwrap(),
            b"\xFF\xFEa\x00\x36\x04"
        );
        assert_eq!(
            encode(&text, Encoding::Utf16Be, false).unwrap(),
            b"\x00a\x04\x36"
        );
        let emoji = Rope::from_str("😀");
        assert_eq!(
            encode(&emoji, Encoding::Utf16Le, false).unwrap(),
            b"\x3D\xD8\x00\xDE"
        );
    }

    #[test]
    fn lists_unencodable_characters_with_offsets() {
        let text = Rope::from_str("Привет, Grüße 😀!");
        let error = encode(&text, Encoding::Legacy("windows-1251"), false).unwrap_err();
        assert_eq!(error.count, 3);
        let chars: Vec<char> = error.samples.iter().map(|(_, ch)| *ch).collect();
        assert_eq!(chars, ['ü', 'ß', '😀']);
        for (offset, ch) in &error.samples {
            assert_eq!(
                text.slice(*offset..*offset + ch.len_utf8()),
                ch.to_string().as_str()
            );
        }
        assert!(
            error
                .to_string()
                .starts_with("3 characters cannot be saved in Windows-1251: 'ü' (U+00FC)")
        );
        let russian = Rope::from_str("Привет");
        assert!(check_encodable(&russian, Encoding::Legacy("windows-1251")).is_ok());
    }

    #[test]
    fn the_message_lists_ten_characters_and_says_when_there_are_more() {
        let latin1 = Encoding::Legacy("windows-1252");
        // One: no plural.
        let one = encode(&Rope::from_str("a\u{0416}"), latin1, false).unwrap_err();
        assert!(one.to_string().starts_with("1 character cannot"), "{one}");
        // Exactly ten: all shown, no ellipsis.
        let ten = encode(&Rope::from_str(&"\u{0416}".repeat(10)), latin1, false).unwrap_err();
        assert_eq!((ten.count, ten.samples.len()), (10, 10));
        assert!(!ten.to_string().ends_with("..."), "{ten}");
        // Eleven, after a long text: ten shown, then an ellipsis.
        let long = format!("{}{}", "x".repeat(100_000), "\u{0416}".repeat(11));
        let eleven = encode(&Rope::from_str(&long), latin1, false).unwrap_err();
        assert_eq!((eleven.count, eleven.samples.len()), (11, 10));
        assert!(eleven.to_string().ends_with(", ..."), "{eleven}");
        assert_eq!(eleven.samples[0].0, 100_000);
    }

    #[test]
    fn empty_text_encodes_to_nothing_or_a_bom() {
        let empty = Rope::new();
        assert_eq!(encode(&empty, Encoding::Utf8, false).unwrap(), b"");
        assert_eq!(
            encode(&empty, Encoding::Utf8, true).unwrap(),
            b"\xEF\xBB\xBF"
        );
        assert_eq!(
            encode(&empty, Encoding::Utf16Be, true).unwrap(),
            b"\xFE\xFF"
        );
        // A legacy encoding has no BOM to write.
        assert_eq!(
            encode(&empty, Encoding::Legacy("windows-1251"), true).unwrap(),
            b""
        );
    }

    #[test]
    fn stateful_encodings_end_in_their_initial_state() {
        // ISO-2022-JP switches to Japanese and must switch back at the end.
        let bytes = encode(
            &Rope::from_str("a日本"),
            Encoding::Legacy("ISO-2022-JP"),
            false,
        )
        .unwrap();
        assert!(bytes.ends_with(b"\x1B(B"), "{bytes:?}");
    }
}
