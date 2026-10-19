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
