//! The code pages encoding_rs does not have (`IBM437`, the EBCDIC pages, ...), through
//! [`decode`] and [`encode`].

use birchpad_core::{Encoding, Rope};
use birchpad_io::*;

fn page(name: &'static str) -> Encoding {
    Encoding::Legacy(name)
}

fn decoded(bytes: &[u8], name: &'static str) -> Decoded {
    decode(bytes.to_vec(), page(name))
}

fn encoded(text: &str, name: &'static str) -> Result<Vec<u8>, Unencodable> {
    encode(&Rope::from_str(text), page(name), false)
}

/// The code pages of Encoding > Character Sets that are Birchpad's own, not encoding_rs's.
fn own_pages() -> impl Iterator<Item = &'static str> {
    CHARACTER_SETS
        .iter()
        .flat_map(|(_, sets)| sets.iter())
        .map(|(name, _)| *name)
        .filter(|name| encoding_rs::Encoding::for_label(name.as_bytes()).is_none())
}

#[test]
fn known_characters() {
    let ibm437 = decoded(&[b'A', 0x82, 0xC9, 0xDB], "IBM437");
    assert_eq!((ibm437.text.as_str(), ibm437.problem), ("Aé╔█", None));
    // EBCDIC: "Hi" is 0xC8 0x89.
    assert_eq!(decoded(&[0xC8, 0x89, 0x25], "IBM037").text, "Hi\n");
    assert_eq!(encoded("Hi\n", "IBM037").unwrap(), [0xC8, 0x89, 0x25]);
    assert_eq!(encoded("€", "IBM1140").unwrap(), [0x9F]);
}

#[test]
fn undefined_bytes_and_unmappable_characters() {
    // A byte IBM857 leaves undefined.
    let undefined = (0..=255u8)
        .find(|&byte| decoded(&[byte], "IBM857").problem.is_some())
        .expect("IBM857 leaves some bytes undefined");
    let text = decoded(&[b'a', undefined, b'b'], "IBM857");
    assert_eq!(text.text, "a\u{FFFD}b");
    assert_eq!(text.problem, Some(DecodeProblem::Malformed { offset: 1 }));
    // Only the undefined byte: the problem is at the start.
    assert_eq!(
        decoded(&[undefined], "IBM857").problem,
        Some(DecodeProblem::Malformed { offset: 0 })
    );
    let refused = encoded("a€b", "IBM437").unwrap_err();
    assert_eq!((refused.count, refused.samples), (1, vec![(1, '€')]));
}

#[test]
fn long_texts_decode_and_encode_whole() {
    // Longer than any buffer the conversion starts with, so that it stops and resumes.
    for length in [0, 1, 4096, 100_000] {
        let bytes = vec![0x82; length];
        let text = decoded(&bytes, "IBM850");
        assert_eq!(text.text, "é".repeat(length), "{length}");
        assert_eq!(text.problem, None);
        assert_eq!(encoded(&text.text, "IBM850").unwrap(), bytes);
    }
}

#[test]
fn every_page_round_trips_its_defined_bytes() {
    let mut pages = 0;
    for name in own_pages() {
        pages += 1;
        for byte in 0..=255u8 {
            let text = decoded(&[byte], name);
            if text.problem.is_some() {
                assert_eq!(text.text, "\u{FFFD}", "{name} {byte:#04X}");
                continue;
            }
            assert_eq!(
                text.text.chars().count(),
                1,
                "{name}: one byte, one character"
            );
            assert_eq!(
                encoded(&text.text, name).unwrap(),
                [byte],
                "{name} {byte:#04X}"
            );
        }
    }
    assert!(pages >= 10, "the OEM and EBCDIC pages are listed");
}

#[test]
fn labels() {
    for label in ["IBM437", "ibm437", "cp437", "CP437", "ibm-437", " cp437 "] {
        assert_eq!(
            parse_encoding(label, Encoding::Utf8),
            Some((page("IBM437"), false)),
            "{label}"
        );
    }
    for label in ["", "437", "cp", "cp99999", "ibm-x", "windows-437"] {
        assert_eq!(parse_encoding(label, Encoding::Utf8), None, "{label}");
    }
    assert_eq!(
        parse_encoding("ibm1140", Encoding::Utf8),
        Some((page("IBM1140"), false))
    );
    // encoding_rs's own: its name, not a code page of Birchpad's.
    assert_eq!(
        parse_encoding("cp866", Encoding::Utf8),
        Some((page("IBM866"), false))
    );
}
