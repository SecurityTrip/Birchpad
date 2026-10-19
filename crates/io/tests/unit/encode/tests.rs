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
