//! Guessing the encoding of a file.
//!
//! Order, first match wins:
//!
//! 1. a byte order mark (UTF-8, UTF-16 LE, UTF-16 BE);
//! 2. valid UTF-8, unless the zero bytes form a UTF-16 pattern (ASCII text in UTF-16 is
//!    technically valid UTF-8 full of NULs);
//! 3. UTF-16 without BOM, recognized by zero bytes at every other position;
//! 4. chardetng's guess among legacy encodings, if the text has enough non-ASCII bytes to judge
//!    and decodes without errors;
//! 5. the system ANSI code page.

use birchpad_core::Encoding;
use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};

use crate::decode::sniff_bom;
use crate::encoding::{from_encoding_rs, to_codec};

/// Bytes inspected for the UTF-16 pattern.
const UTF16_SAMPLE: usize = 64 * 1024;
/// Bytes given to chardetng; it is fast, and more text makes better guesses.
const GUESS_SAMPLE: usize = 4 * 1024 * 1024;
/// With fewer non-ASCII bytes than this, a guess is a coin toss and the ANSI code page is the
/// better bet (a file with one "é" on a Western system is Windows-1252).
const MIN_NON_ASCII: usize = 16;

/// Bytes that are Ukrainian letters in KOI8-U and box-drawing characters in KOI8-R.
const KOI8_U_ONLY: [u8; 8] = [0xA4, 0xA6, 0xA7, 0xAD, 0xB4, 0xB6, 0xB7, 0xBD];

/// Which rule picked the encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectedBy {
    Bom,
    Utf8,
    Utf16Pattern,
    Guess,
    Ansi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Detection {
    pub encoding: Encoding,
    pub by: DetectedBy,
}

/// Guesses the encoding of `bytes`. `ansi` is the fallback legacy encoding.
pub fn detect(bytes: &[u8], ansi: Encoding) -> Detection {
    let found = |encoding, by| Detection { encoding, by };
    if let Some(encoding) = sniff_bom(bytes) {
        return found(encoding, DetectedBy::Bom);
    }
    let utf16 = utf16_pattern(&bytes[..bytes.len().min(UTF16_SAMPLE)]);
    if utf16.is_none() && std::str::from_utf8(bytes).is_ok() {
        return found(Encoding::Utf8, DetectedBy::Utf8);
    }
    if let Some(encoding) = utf16 {
        return found(encoding, DetectedBy::Utf16Pattern);
    }

    let sample = &bytes[..bytes.len().min(GUESS_SAMPLE)];
    let non_ascii = sample.iter().filter(|byte| !byte.is_ascii()).count();
    let mut detector = EncodingDetector::new(Iso2022JpDetection::Deny);
    detector.feed(sample, sample.len() == bytes.len());
    let tld = locale_hint(ansi);
    let guess = from_encoding_rs(detector.guess(tld, Utf8Detection::Deny)).map(|guess| {
        // chardetng reports KOI8-U for all KOI8 text. Without the bytes where KOI8-U has
        // Ukrainian letters (KOI8-R has box drawing there), KOI8-R is the better name.
        if guess == Encoding::Legacy("KOI8-U") && !sample.iter().any(|b| KOI8_U_ONLY.contains(b)) {
            Encoding::Legacy("KOI8-R")
        } else {
            guess
        }
    });

    match guess {
        Some(guess)
            if guess != ansi
                && non_ascii >= MIN_NON_ASCII
                && decodes_cleanly(sample, guess, sample.len() == bytes.len()) =>
        {
            found(guess, DetectedBy::Guess)
        }
        _ => found(ansi, DetectedBy::Ansi),
    }
}

/// chardetng was made for the Web, where the top-level domain hints at the language. A desktop
/// editor has a better hint: the system's ANSI code page tells which legacy encodings are common
/// on this machine.
fn locale_hint(ansi: Encoding) -> Option<&'static [u8]> {
    let Encoding::Legacy(name) = ansi else {
        return None;
    };
    let tld: &'static [u8] = match name {
        "Shift_JIS" => b"jp",
        "GBK" | "gb18030" => b"cn",
        "Big5" => b"tw",
        "EUC-KR" => b"kr",
        "windows-1251" | "IBM866" | "KOI8-R" => b"ru",
        "windows-1250" => b"pl",
        "windows-1253" => b"gr",
        "windows-1254" => b"tr",
        "windows-1255" => b"il",
        "windows-1256" => b"sa",
        "windows-1257" => b"lt",
        "windows-1258" => b"vn",
        "windows-874" => b"th",
        _ => return None,
    };
    Some(tld)
}

/// UTF-16 LE or BE if ASCII-range characters show up as one zero byte in every pair.
fn utf16_pattern(sample: &[u8]) -> Option<Encoding> {
    let pairs = sample.len() / 2;
    if pairs < 2 {
        return None;
    }
    let (mut even_zero, mut odd_zero) = (0, 0);
    for [even, odd] in sample.as_chunks::<2>().0 {
        even_zero += usize::from(*even == 0);
        odd_zero += usize::from(*odd == 0);
    }
    // At least 40% of the pairs zero in one half, almost none in the other.
    let mostly = |count: usize| count * 10 >= pairs * 4;
    let rarely = |count: usize| count * 20 < pairs;
    let encoding = if mostly(odd_zero) && rarely(even_zero) {
        Encoding::Utf16Le
    } else if mostly(even_zero) && rarely(odd_zero) {
        Encoding::Utf16Be
    } else {
        return None;
    };
    let whole_units = &sample[..pairs * 2];
    decodes_cleanly(whole_units, encoding, false).then_some(encoding)
}

/// True if `bytes` decode without errors. If `complete` is false, `bytes` is a prefix and may
/// end in the middle of a character.
fn decodes_cleanly(bytes: &[u8], encoding: Encoding, complete: bool) -> bool {
    let mut decoder = to_codec(encoding).new_decoder_without_bom_handling();
    let mut out = String::with_capacity(
        decoder
            .max_utf8_buffer_length_without_replacement(bytes.len())
            .unwrap_or(bytes.len() * 3),
    );
    let (result, _) = decoder.decode_to_string_without_replacement(bytes, &mut out, complete);
    matches!(result, encoding_rs::DecoderResult::InputEmpty)
}
