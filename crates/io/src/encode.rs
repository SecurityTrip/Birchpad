//! Text to bytes. Encoding never replaces characters: if the text cannot be represented, the
//! caller gets the list of offending characters and nothing is written.

use std::fmt;

use birchpad_core::{Encoding, Format, LineEnding, Rope};
use encoding_rs::EncoderResult;

use crate::decode::bom_bytes;
use crate::encoding::{display_name, to_codec};

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
        let more = if self.count > self.samples.len() {
            ", ..."
        } else {
            ""
        };
        f.write_str(&birchpad_i18n::t!(
            "io.unencodable",
            characters = birchpad_i18n::count(self.count, "character", "characters"),
            encoding = display_name(self.encoding, false),
            samples = format!("{}{more}", chars.join(", "))
        ))
    }
}

impl std::error::Error for Unencodable {}

/// Whether `encoding` is an EBCDIC code page: one where NEL (0x15) and LF (0x25) are the line
/// breaks.
pub fn is_ebcdic(encoding: Encoding) -> bool {
    let Encoding::Legacy(name) = encoding else {
        return false;
    };
    crate::codec::code_page(name)
        .is_some_and(|page| page.chars[0x15] == 0x85 && page.chars[0x25] == 0x0A)
}

/// Encodes a document's `text` in its `format`: [`encode`], with the line breaks of an EBCDIC
/// document whose line ending is NEL written as NEL.
pub fn encode_document(text: &Rope, format: Format) -> Result<Vec<u8>, Unencodable> {
    let mut bytes = encode(text, format.encoding, format.bom)?;
    if format.line_ending == LineEnding::Nel && is_ebcdic(format.encoding) {
        // Only LF becomes 0x25 in an EBCDIC code page.
        for byte in &mut bytes {
            if *byte == 0x25 {
                *byte = 0x15;
            }
        }
    }
    Ok(bytes)
}

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
    let mut encoder = to_codec(encoding).new_encoder();
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
