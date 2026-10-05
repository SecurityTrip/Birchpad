//! Where saving a file that did not decode exactly would write other bytes than it has: malformed
//! bytes, read as U+FFFD, and byte sequences of legacy encodings that encode back differently.
//! Shown before such a file is edited anyway.

use birchpad_core::{Encoding, Rope};
use encoding_rs::{DecoderResult, EncoderResult};

use crate::encode::encode;
use crate::encoding::to_encoding_rs;

/// How many changes are kept to be listed.
pub const LISTED: usize = 8;

/// A place where saving writes other bytes than the file has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByteChange {
    /// Offset in the file.
    pub offset: usize,
    /// The file's bytes there.
    pub before: Vec<u8>,
    /// The text they were read as: U+FFFD for malformed bytes.
    pub text: String,
    /// What saving writes instead; `None` if the encoding cannot write `text` (saving is refused
    /// until it is replaced).
    pub after: Option<Vec<u8>>,
}

/// All the places where saving changes a file, and the first [`LISTED`] of them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ByteChanges {
    pub count: usize,
    /// How many of them the encoding cannot write.
    pub unwritable: usize,
    pub first: Vec<ByteChange>,
}

impl ByteChanges {
    fn add(&mut self, offset: usize, before: &[u8], text: &str, after: Option<Vec<u8>>) {
        self.count += 1;
        if after.is_none() {
            self.unwritable += 1;
        }
        if self.first.len() < LISTED {
            self.first.push(ByteChange {
                offset,
                before: before.to_vec(),
                text: text.to_owned(),
                after,
            });
        }
    }

    /// A malformed sequence, read as U+FFFD.
    fn malformed(&mut self, offset: usize, before: &[u8], encoding: Encoding) {
        let after = encode(&Rope::from_str("\u{FFFD}"), encoding, false).ok();
        self.add(offset, before, "\u{FFFD}", after);
    }
}

/// The places where encoding the decoded text of `bytes` (a file's bytes after its byte order
/// mark, which takes `skip` bytes) in `encoding` gives other bytes.
pub fn byte_changes(bytes: &[u8], skip: usize, encoding: Encoding) -> ByteChanges {
    let mut changes = ByteChanges::default();
    match encoding {
        Encoding::Utf8 => {
            let mut pos = 0;
            while let Err(error) = std::str::from_utf8(&bytes[pos..]) {
                let start = pos + error.valid_up_to();
                let end = error.error_len().map_or(bytes.len(), |len| start + len);
                changes.malformed(skip + start, &bytes[start..end], encoding);
                pos = end;
            }
        }
        Encoding::Legacy(_) if !to_encoding_rs(encoding).is_single_byte() => {
            multi_byte(bytes, skip, encoding, &mut changes);
        }
        // UTF-16 and single-byte encodings map bytes to characters one to one: only malformed
        // bytes change.
        _ => {
            let mut decoder = to_encoding_rs(encoding).new_decoder_without_bom_handling();
            let mut text = String::with_capacity(64 * 1024);
            let mut pos = 0;
            loop {
                text.clear();
                let (result, read) =
                    decoder.decode_to_string_without_replacement(&bytes[pos..], &mut text, true);
                pos += read;
                match result {
                    DecoderResult::InputEmpty => break,
                    DecoderResult::OutputFull => {}
                    DecoderResult::Malformed(bad, after) => {
                        let end = pos - after as usize;
                        let start = end - bad as usize;
                        changes.malformed(skip + start, &bytes[start..end], encoding);
                    }
                }
            }
        }
    }
    changes
}

/// Multi-byte legacy encodings. Lines that decode and encode back to themselves are skipped;
/// the others are decoded byte by byte, which tells which bytes each character came from.
/// ISO-2022-JP switches modes with escape sequences that carry over lines, so it is decoded
/// byte by byte throughout.
fn multi_byte(bytes: &[u8], skip: usize, encoding: Encoding, changes: &mut ByteChanges) {
    let target = to_encoding_rs(encoding);
    if target == encoding_rs::ISO_2022_JP {
        by_character(bytes, skip, encoding, changes);
        return;
    }
    let mut offset = 0;
    // A line break is never part of a multi-byte character in these encodings.
    for line in bytes.split_inclusive(|&byte| byte == b'\n') {
        let same = target
            .decode_without_bom_handling_and_without_replacement(line)
            .is_some_and(|text| {
                let (again, _, unmappable) = target.encode(&text);
                !unmappable && again == line
            });
        if !same {
            by_character(line, skip + offset, encoding, changes);
        }
        offset += line.len();
    }
}

/// Decodes `bytes` (at file offset `base`) one byte at a time, and compares the bytes of each
/// character with how it encodes.
fn by_character(bytes: &[u8], base: usize, encoding: Encoding, changes: &mut ByteChanges) {
    let target = to_encoding_rs(encoding);
    let mut decoder = target.new_decoder_without_bom_handling();
    let mut encoder = target.new_encoder();
    let mut text = String::with_capacity(32);
    let mut again = Vec::with_capacity(32);
    // Bytes since the last character, which make the next one.
    let mut start = 0;
    let mut pos = 0;
    let mut compare =
        |changes: &mut ByteChanges, start: usize, end: usize, text: &str, last: bool| {
            again.clear();
            again.reserve(32);
            let (result, _) =
                encoder.encode_from_utf8_to_vec_without_replacement(text, &mut again, last);
            let after = match result {
                EncoderResult::Unmappable(_) => None,
                _ => Some(again.clone()),
            };
            if after.as_deref() != Some(&bytes[start..end]) {
                changes.add(base + start, &bytes[start..end], text, after);
            }
        };
    loop {
        let last = pos + 1 >= bytes.len();
        let input = &bytes[pos..(pos + 1).min(bytes.len())];
        text.clear();
        let (result, read) = decoder.decode_to_string_without_replacement(input, &mut text, last);
        pos += read;
        match result {
            DecoderResult::Malformed(bad, after) => {
                let end = pos - after as usize;
                let bad_start = end - bad as usize;
                if !text.is_empty() {
                    compare(changes, start, bad_start, &text, false);
                }
                changes.malformed(base + bad_start, &bytes[bad_start..end], encoding);
                start = end;
            }
            DecoderResult::InputEmpty | DecoderResult::OutputFull => {
                if !text.is_empty() {
                    compare(changes, start, pos, &text, false);
                    start = pos;
                }
                if pos >= bytes.len() && last {
                    break;
                }
            }
        }
    }
    // Escape sequences after the last character, and the encoder's closing one.
    compare(changes, start, bytes.len(), "", true);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(offset: usize, before: &[u8], text: &str, after: Option<&[u8]>) -> ByteChange {
        ByteChange {
            offset,
            before: before.to_vec(),
            text: text.to_owned(),
            after: after.map(<[u8]>::to_vec),
        }
    }

    #[test]
    fn malformed_utf8_becomes_replacement_characters() {
        let changes = byte_changes(b"ok \xC3( \xFF\xFE end \xE2\x82", 3, Encoding::Utf8);
        let replacement = Some(&b"\xEF\xBF\xBD"[..]);
        assert_eq!(
            changes.first,
            [
                change(6, b"\xC3", "\u{FFFD}", replacement),
                change(9, b"\xFF", "\u{FFFD}", replacement),
                change(10, b"\xFE", "\u{FFFD}", replacement),
                change(16, b"\xE2\x82", "\u{FFFD}", replacement),
            ]
        );
        assert_eq!((changes.count, changes.unwritable), (4, 0));
    }

    #[test]
    fn many_changes_are_counted_and_the_first_listed() {
        let changes = byte_changes(&[0xFF; 100], 0, Encoding::Utf8);
        assert_eq!((changes.count, changes.first.len()), (100, LISTED));
    }

    #[test]
    fn utf16_and_single_byte_encodings_change_only_malformed_bytes() {
        // An unpaired surrogate.
        let changes = byte_changes(b"a\0\x00\xD8b\0", 2, Encoding::Utf16Le);
        assert_eq!(
            changes.first,
            [change(4, b"\x00\xD8", "\u{FFFD}", Some(b"\xFD\xFF"))]
        );
        // Windows-1253 has no character at 0xAA, and no U+FFFD to write in its place.
        let changes = byte_changes(b"a\xAAb", 0, Encoding::Legacy("windows-1253"));
        assert_eq!(changes.first, [change(1, b"\xAA", "\u{FFFD}", None)]);
        assert_eq!(changes.unwritable, 1);
        assert_eq!(
            byte_changes(b"\xC0\xFF", 0, Encoding::Legacy("windows-1251")).count,
            0
        );
    }

    #[test]
    fn legacy_sequences_that_encode_differently() {
        // Shift_JIS: 0xFA5B (an IBM extension) reads as ∵, which encodes as 0x81E6; a lead byte
        // without its trail is malformed.
        let bytes = b"ab\xFA\x5B\n\x82\xA0ok\nx\x82";
        let changes = byte_changes(bytes, 0, Encoding::Legacy("Shift_JIS"));
        assert_eq!(
            changes.first,
            [
                change(2, b"\xFA\x5B", "∵", Some(b"\x81\xE6")),
                change(11, b"\x82", "\u{FFFD}", None),
            ]
        );
    }

    #[test]
    fn iso_2022_jp_compares_characters_with_their_escapes() {
        // ESC ( J switches to JIS X 0201 Roman, which the encoder writes as ASCII.
        let bytes = b"a\x1B(Jb\x1B(Bc";
        let changes = byte_changes(bytes, 0, Encoding::Legacy("ISO-2022-JP"));
        assert!(changes.count > 0);
        assert_eq!(changes.first[0].offset, 1);
        let clean = b"a\x1B$B\x30\x21\x1B(Bc";
        assert_eq!(
            byte_changes(clean, 0, Encoding::Legacy("ISO-2022-JP")).count,
            0
        );
    }
}
