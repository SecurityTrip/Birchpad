//! Property tests: decoding and encoding are exact inverses wherever they report no problem.

use birchpad_core::{Encoding, Rope};
use birchpad_io::{CHARACTER_SETS, decode, encode};
use proptest::prelude::*;

fn legacy_encodings() -> Vec<Encoding> {
    CHARACTER_SETS
        .iter()
        .flat_map(|(_, sets)| sets.iter())
        .map(|(name, _)| Encoding::Legacy(name))
        .collect()
}

fn all_encodings() -> Vec<Encoding> {
    let mut all = vec![Encoding::Utf8, Encoding::Utf16Le, Encoding::Utf16Be];
    all.extend(legacy_encodings());
    all
}

/// Text mixing scripts so that every encoding sees both characters it has and ones it lacks.
fn text() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::sample::select(vec![
            "a", "Z", " ", "\r\n", "\n", "\r", "\t", "\0", "é", "ß", "ж", "Ё", "ї", "Ω", "א", "ب",
            "ก", "ă", "€", "‚", "日", "本", "語", "い", "한", "中", "😀", "\u{FEFF}",
        ]),
        0..64,
    )
    .prop_map(|parts| parts.concat())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Any byte string that decodes without a reported problem encodes back to itself.
    #[test]
    fn clean_decoding_round_trips(
        bytes in prop::collection::vec(any::<u8>(), 0..256),
        encoding in prop::sample::select(all_encodings()),
    ) {
        let decoded = decode(bytes.clone(), encoding);
        if decoded.problem.is_none() {
            let again = encode(&Rope::from_str(&decoded.text), encoding, decoded.bom).unwrap();
            prop_assert_eq!(again, bytes);
        }
    }

    /// Any text that encodes without error decodes back to itself.
    #[test]
    fn encodable_text_round_trips(
        text in text(),
        encoding in prop::sample::select(all_encodings()),
        bom: bool,
    ) {
        let bom = bom && encoding.is_unicode();
        if let Ok(bytes) = encode(&Rope::from_str(&text), encoding, bom) {
            let decoded = decode(bytes, encoding);
            prop_assert_eq!(decoded.problem, None);
            // A text starting with U+FEFF and written without BOM reads back as having a BOM:
            // that is what the bytes say.
            if !text.starts_with('\u{FEFF}') || bom {
                prop_assert_eq!(decoded.bom, bom);
                prop_assert_eq!(decoded.text, text);
            }
        }
    }

    /// Unicode encodings represent every text.
    #[test]
    fn unicode_encodings_never_fail(text in text(), bom: bool) {
        for encoding in [Encoding::Utf8, Encoding::Utf16Le, Encoding::Utf16Be] {
            prop_assert!(encode(&Rope::from_str(&text), encoding, bom).is_ok());
        }
    }
}

/// Single-byte encodings: every byte that decodes to a character encodes back to that byte.
#[test]
fn single_byte_encodings_are_bijective() {
    for encoding in legacy_encodings() {
        let all: Vec<u8> = (0..=255).collect();
        let decoded = decode(all.clone(), encoding);
        if decoded.text.chars().count() != 256 {
            continue; // multi-byte encoding
        }
        for byte in 0..=255u8 {
            let decoded = decode(vec![byte], encoding);
            if decoded.problem.is_none() {
                let again = encode(&Rope::from_str(&decoded.text), encoding, false).unwrap();
                assert_eq!(again, [byte], "{encoding:?} byte {byte:#04X}");
            }
        }
    }
}
