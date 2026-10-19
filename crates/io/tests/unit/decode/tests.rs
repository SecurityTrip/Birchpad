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
