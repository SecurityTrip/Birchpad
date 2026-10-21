//! A legacy encoding's decoder and encoder: encoding_rs's, or one of the code pages it lacks
//! ([`crate::code_pages`]: DOS and EBCDIC), behind the same calls, so that reading, saving,
//! detection and the list of changed bytes work the same for both.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::OnceLock;

use encoding_rs::{DecoderResult, EncoderResult};

use crate::code_pages::{CODE_PAGES, CodePage};

/// A byte the code page leaves undefined.
const UNDEFINED: u16 = 0xFFFF;

#[derive(Clone, Copy)]
pub(crate) enum Codec {
    Rs(&'static encoding_rs::Encoding),
    Page(&'static CodePage),
}

/// The code page named `name` (`IBM437`), ignoring case.
pub(crate) fn code_page(name: &str) -> Option<&'static CodePage> {
    CODE_PAGES
        .iter()
        .find(|page| page.name.eq_ignore_ascii_case(name))
}

/// The code page a label names: `IBM437`, `cp437`, `ibm-437`.
pub(crate) fn code_page_for_label(label: &str) -> Option<&'static CodePage> {
    let lower = label.trim().to_ascii_lowercase();
    let number = lower
        .strip_prefix("ibm-")
        .or_else(|| lower.strip_prefix("ibm"))
        .or_else(|| lower.strip_prefix("cp"))?;
    let number: u32 = number.parse().ok()?;
    CODE_PAGES.iter().find(|page| page.number == number)
}

fn char_of(page: &CodePage, byte: u8) -> Option<char> {
    let unit = page.chars[usize::from(byte)];
    (unit != UNDEFINED)
        .then(|| char::from_u32(u32::from(unit)))
        .flatten()
}

/// Each code page's characters to their bytes.
fn reverse(page: &'static CodePage) -> &'static HashMap<char, u8> {
    static MAPS: OnceLock<Vec<HashMap<char, u8>>> = OnceLock::new();
    let maps = MAPS.get_or_init(|| {
        CODE_PAGES
            .iter()
            .map(|page| {
                (0..=255u8)
                    .filter_map(|byte| char_of(page, byte).map(|ch| (ch, byte)))
                    .collect()
            })
            .collect()
    });
    let index = CODE_PAGES
        .iter()
        .position(|known| std::ptr::eq(known, page))
        .expect("a code page of the table");
    &maps[index]
}

impl Codec {
    pub(crate) fn is_single_byte(self) -> bool {
        match self {
            Self::Rs(encoding) => encoding.is_single_byte(),
            Self::Page(_) => true,
        }
    }

    /// The text of `bytes`, undefined bytes replaced, and whether there were any.
    pub(crate) fn decode_without_bom_handling(self, bytes: &[u8]) -> (Cow<'_, str>, bool) {
        match self {
            Self::Rs(encoding) => encoding.decode_without_bom_handling(bytes),
            Self::Page(page) => {
                let mut errors = false;
                let text = bytes
                    .iter()
                    .map(|&byte| {
                        char_of(page, byte).unwrap_or_else(|| {
                            errors = true;
                            char::REPLACEMENT_CHARACTER
                        })
                    })
                    .collect::<String>();
                (Cow::Owned(text), errors)
            }
        }
    }

    pub(crate) fn new_decoder_without_bom_handling(self) -> Decoder {
        match self {
            Self::Rs(encoding) => Decoder::Rs(encoding.new_decoder_without_bom_handling()),
            Self::Page(page) => Decoder::Page(page),
        }
    }

    pub(crate) fn new_encoder(self) -> Encoder {
        match self {
            Self::Rs(encoding) => Encoder::Rs(encoding.new_encoder()),
            Self::Page(page) => Encoder::Page(page),
        }
    }
}

pub(crate) enum Decoder {
    Rs(encoding_rs::Decoder),
    Page(&'static CodePage),
}

impl Decoder {
    pub(crate) fn max_utf8_buffer_length_without_replacement(&self, len: usize) -> Option<usize> {
        match self {
            Self::Rs(decoder) => decoder.max_utf8_buffer_length_without_replacement(len),
            // Every character of these code pages is in the Basic Multilingual Plane.
            Self::Page(_) => len.checked_mul(3),
        }
    }

    /// As encoding_rs's: decodes into the free capacity of `dst`, stops at an undefined byte
    /// (`Malformed(1, 0)`, counted as read) or when `dst` is full.
    pub(crate) fn decode_to_string_without_replacement(
        &mut self,
        src: &[u8],
        dst: &mut String,
        last: bool,
    ) -> (DecoderResult, usize) {
        match self {
            Self::Rs(decoder) => decoder.decode_to_string_without_replacement(src, dst, last),
            Self::Page(page) => {
                for (index, &byte) in src.iter().enumerate() {
                    let Some(ch) = char_of(page, byte) else {
                        return (DecoderResult::Malformed(1, 0), index + 1);
                    };
                    if dst.capacity() - dst.len() < ch.len_utf8() {
                        return (DecoderResult::OutputFull, index);
                    }
                    dst.push(ch);
                }
                (DecoderResult::InputEmpty, src.len())
            }
        }
    }
}

pub(crate) enum Encoder {
    Rs(encoding_rs::Encoder),
    Page(&'static CodePage),
}

impl Encoder {
    pub(crate) fn max_buffer_length_from_utf8_without_replacement(
        &self,
        len: usize,
    ) -> Option<usize> {
        match self {
            Self::Rs(encoder) => encoder.max_buffer_length_from_utf8_without_replacement(len),
            Self::Page(_) => Some(len),
        }
    }

    /// As encoding_rs's: encodes into the free capacity of `dst`, stops at a character without
    /// a byte (`Unmappable`, counted as read) or when `dst` is full.
    pub(crate) fn encode_from_utf8_to_vec_without_replacement(
        &mut self,
        src: &str,
        dst: &mut Vec<u8>,
        last: bool,
    ) -> (EncoderResult, usize) {
        match self {
            Self::Rs(encoder) => {
                encoder.encode_from_utf8_to_vec_without_replacement(src, dst, last)
            }
            Self::Page(page) => {
                let map = reverse(page);
                for (index, ch) in src.char_indices() {
                    let Some(&byte) = map.get(&ch) else {
                        return (EncoderResult::Unmappable(ch), index + ch.len_utf8());
                    };
                    if dst.len() == dst.capacity() {
                        return (EncoderResult::OutputFull, index);
                    }
                    dst.push(byte);
                }
                (EncoderResult::InputEmpty, src.len())
            }
        }
    }
}
