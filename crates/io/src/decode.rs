//! Bytes to text, reporting every reason the text could not be saved back unchanged.

use birchpad_core::Encoding;
use encoding_rs::DecoderResult;

use crate::changes::{ByteChanges, byte_changes};
use crate::encode;
use crate::encoding::to_encoding_rs;

/// Why a decoded file cannot be edited safely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeProblem {
    /// Some bytes are not valid in the encoding; they were replaced by U+FFFD. `offset` is the
    /// file position of the first one.
    Malformed { offset: usize },
    /// The bytes decode, but encoding the text again would not give the same bytes (several
    /// byte sequences map to one character in some legacy encodings). `offset` is the file
    /// position of the first difference.
    NotReversible { offset: usize },
}

impl DecodeProblem {
    pub fn offset(self) -> usize {
        match self {
            Self::Malformed { offset } | Self::NotReversible { offset } => offset,
        }
    }
}

/// The result of decoding a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub text: String,
    pub encoding: Encoding,
    /// The file started with the byte order mark of `encoding`, which is not part of `text`.
    pub bom: bool,
    pub problem: Option<DecodeProblem>,
    /// With a problem: where saving the text would change the file.
    pub changes: ByteChanges,
}

/// The byte order mark of a Unicode encoding.
pub fn bom_bytes(encoding: Encoding) -> &'static [u8] {
    match encoding {
        Encoding::Utf8 => b"\xEF\xBB\xBF",
        Encoding::Utf16Le => b"\xFF\xFE",
        Encoding::Utf16Be => b"\xFE\xFF",
        Encoding::Legacy(_) => b"",
    }
}

/// The Unicode encoding whose byte order mark starts `bytes`, if any.
pub fn sniff_bom(bytes: &[u8]) -> Option<Encoding> {
    [Encoding::Utf8, Encoding::Utf16Le, Encoding::Utf16Be]
        .into_iter()
        .find(|&encoding| bytes.starts_with(bom_bytes(encoding)))
}

/// Decodes `bytes` as `encoding`. A leading byte order mark of that encoding is removed and
/// reported in [`Decoded::bom`]; it is never treated as text.
pub fn decode(mut bytes: Vec<u8>, encoding: Encoding) -> Decoded {
    let bom = bom_bytes(encoding);
    let has_bom = !bom.is_empty() && bytes.starts_with(bom);
    let skip = if has_bom { bom.len() } else { 0 };

    let mut changes = ByteChanges::default();
    let (text, problem) = match encoding {
        Encoding::Utf8 => {
            bytes.drain(..skip);
            match String::from_utf8(bytes) {
                Ok(text) => (text, None),
                Err(error) => {
                    let offset = skip + error.utf8_error().valid_up_to();
                    let text = String::from_utf8_lossy(error.as_bytes()).into_owned();
                    changes = byte_changes(error.as_bytes(), skip, encoding);
                    (text, Some(DecodeProblem::Malformed { offset }))
                }
            }
        }
        _ => {
            let (text, malformed) = decode_with(encoding, &bytes[skip..]);
            let problem = malformed
                .map(|offset| DecodeProblem::Malformed {
                    offset: skip + offset,
                })
                .or_else(|| {
                    needs_round_trip_check(encoding)
                        .then(|| first_round_trip_difference(&text, encoding, &bytes[skip..]))
                        .flatten()
                        .map(|offset| DecodeProblem::NotReversible {
                            offset: skip + offset,
                        })
                });
            if problem.is_some() {
                changes = byte_changes(&bytes[skip..], skip, encoding);
            }
            (text, problem)
        }
    };
    Decoded {
        text,
        encoding,
        bom: has_bom,
        problem,
        changes,
    }
}

/// Decodes with encoding_rs, replacing malformed sequences. Returns the offset of the first
/// malformed byte, if any.
fn decode_with(encoding: Encoding, bytes: &[u8]) -> (String, Option<usize>) {
    let target = to_encoding_rs(encoding);
    let (text, had_errors) = target.decode_without_bom_handling(bytes);
    if !had_errors {
        return (text.into_owned(), None);
    }
    // Only now, in the rare error case, pay for finding where the first error is.
    let mut decoder = target.new_decoder_without_bom_handling();
    let capacity = decoder
        .max_utf8_buffer_length_without_replacement(bytes.len())
        .unwrap_or(bytes.len() * 3);
    let mut scratch = String::with_capacity(capacity);
    let (result, read) = decoder.decode_to_string_without_replacement(bytes, &mut scratch, true);
    let offset = match result {
        DecoderResult::Malformed(bad, consumed_after) => {
            read.saturating_sub(bad as usize + consumed_after as usize)
        }
        DecoderResult::InputEmpty | DecoderResult::OutputFull => read,
    };
    (text.into_owned(), Some(offset))
}

/// Single-byte encodings and UTF-16 map bytes to characters one to one, so text that decoded
/// without errors always encodes back to the same bytes. Multi-byte legacy encodings do not.
fn needs_round_trip_check(encoding: Encoding) -> bool {
    match encoding {
        Encoding::Legacy(_) => !to_encoding_rs(encoding).is_single_byte(),
        _ => false,
    }
}

fn first_round_trip_difference(text: &str, encoding: Encoding, bytes: &[u8]) -> Option<usize> {
    let rope = birchpad_core::Rope::from_str(text);
    match encode::encode(&rope, encoding, false) {
        Ok(again) if again == bytes => None,
        Ok(again) => Some(
            again
                .iter()
                .zip(bytes)
                .position(|(a, b)| a != b)
                .unwrap_or(again.len().min(bytes.len())),
        ),
        Err(_) => Some(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_only_a_matching_bom() {
        let decoded = decode(b"\xEF\xBB\xBFhi".to_vec(), Encoding::Utf8);
        assert_eq!((decoded.text.as_str(), decoded.bom), ("hi", true));
        let decoded = decode(b"\xFF\xFEh\0i\0".to_vec(), Encoding::Utf16Le);
        assert_eq!((decoded.text.as_str(), decoded.bom), ("hi", true));
        // A UTF-16 BOM in a file read as UTF-8 is just invalid bytes.
        let decoded = decode(b"\xFF\xFEh\0".to_vec(), Encoding::Utf8);
        assert_eq!(
            decoded.problem,
            Some(DecodeProblem::Malformed { offset: 0 })
        );
    }

    #[test]
    fn a_problem_has_an_offset_of_either_kind() {
        assert_eq!(DecodeProblem::Malformed { offset: 7 }.offset(), 7);
        assert_eq!(DecodeProblem::NotReversible { offset: 0 }.offset(), 0);
    }

    #[test]
    fn locates_the_first_malformed_byte() {
        let decoded = decode(b"ok \xC3( rest".to_vec(), Encoding::Utf8);
        assert_eq!(
            decoded.problem,
            Some(DecodeProblem::Malformed { offset: 3 })
        );
        assert!(decoded.text.contains('\u{FFFD}'));

        // An unpaired surrogate in UTF-16 LE at code unit 2.
        let decoded = decode(b"a\0b\0\x00\xD8c\0".to_vec(), Encoding::Utf16Le);
        assert_eq!(
            decoded.problem,
            Some(DecodeProblem::Malformed { offset: 4 })
        );

        // A truncated final code unit.
        let decoded = decode(b"a\0b".to_vec(), Encoding::Utf16Le);
        assert_eq!(
            decoded.problem,
            Some(DecodeProblem::Malformed { offset: 2 })
        );
    }

    #[test]
    fn single_byte_decoding_has_no_errors_for_defined_bytes() {
        let all: Vec<u8> = (0..=255).collect();
        let decoded = decode(all, Encoding::Legacy("windows-1252"));
        assert_eq!(decoded.problem, None);
        assert_eq!(decoded.text.chars().count(), 256);
    }

    #[test]
    fn reports_bytes_that_would_not_round_trip() {
        // Shift_JIS: 0x81E6 (JIS X 0208) and 0xFA5B (IBM extension) both decode to U+2235 (∵);
        // the encoder only produces 0x81E6.
        let bytes = b"ab\xFA\x5B".to_vec();
        let decoded = decode(bytes, Encoding::Legacy("Shift_JIS"));
        assert!(matches!(
            decoded.problem,
            Some(DecodeProblem::NotReversible { offset: 2 })
        ));
    }
}
