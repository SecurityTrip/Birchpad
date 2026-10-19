use super::*;

#[test]
fn appended_utf8_waits_for_whole_characters() {
    let bytes = "ab ж".as_bytes();
    assert_eq!(
        decode_appended(&bytes[..4], Encoding::Utf8),
        Some(("ab ".to_owned(), 3)),
        "half of ж"
    );
    assert_eq!(
        decode_appended(bytes, Encoding::Utf8),
        Some(("ab ж".to_owned(), 5))
    );
    assert_eq!(decode_appended(b"a\xffb", Encoding::Utf8), None);
}

#[test]
fn appended_utf16_and_single_byte() {
    let mut bytes: Vec<u8> = "a😀".encode_utf16().flat_map(u16::to_le_bytes).collect();
    let whole = bytes.len();
    assert_eq!(
        decode_appended(&bytes, Encoding::Utf16Le),
        Some(("a😀".to_owned(), whole))
    );
    bytes.truncate(whole - 2);
    assert_eq!(
        decode_appended(&bytes, Encoding::Utf16Le),
        Some(("a".to_owned(), 2)),
        "the lead surrogate waits"
    );
    assert_eq!(
        decode_appended(&[0xC0, 0xE4], Encoding::Legacy("windows-1251")),
        Some(("Ад".to_owned(), 2))
    );
    assert_eq!(decode_appended(b"x", Encoding::Legacy("Shift_JIS")), None);
}

#[test]
fn stamps_heads_and_tails() {
    let dir = std::env::temp_dir().join(format!("birchpad-disk-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("log.txt");
    assert_eq!(stamp(&path).unwrap(), None);
    std::fs::write(&path, "first\n").unwrap();
    let head = Head::of(b"first\n");
    assert_eq!(stamp(&path).unwrap().unwrap().len, 6);

    std::fs::write(&path, "first\nsecond\n").unwrap();
    assert!(head.matches(&path).unwrap(), "only appended to");
    assert_eq!(read_from(&path, 6).unwrap(), b"second\n");
    std::fs::write(&path, "other\nsecond\n").unwrap();
    assert!(!head.matches(&path).unwrap(), "rewritten");
    std::fs::write(&path, "fir").unwrap();
    assert!(!head.matches(&path).unwrap(), "truncated");
    std::fs::remove_dir_all(&dir).unwrap();
}
