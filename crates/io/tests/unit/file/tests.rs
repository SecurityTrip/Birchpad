use super::*;

fn ebcdic(bytes: &[u8], name: &'static str) -> LoadedFile {
    let info = FileInfo {
        len: bytes.len() as u64,
        read_only: false,
        modified: None,
    };
    let options = LoadOptions {
        encoding: Some(Encoding::Legacy(name)),
        ansi: Encoding::Legacy("windows-1252"),
    };
    decode_file(bytes.to_vec(), info, options)
}

#[test]
fn ebcdic_lines_broken_with_nel_are_lines() {
    // "Hi" NEL "A" NEL in EBCDIC 037.
    let bytes = [0xC8, 0x89, 0x15, 0xC1, 0x15];
    let file = ebcdic(&bytes, "IBM037");
    assert_eq!(file.text.to_string(), "Hi\nA\n");
    assert_eq!(file.format.line_ending, LineEnding::Nel);
    assert!(file.problem.is_none());
    let doc = birchpad_core::Document::with_format(file.text.clone(), file.format);
    assert_eq!(
        doc.line_ending(),
        LineEnding::Nel,
        "kept although the text has LF"
    );
    assert_eq!(
        crate::encode_document(doc.text(), doc.format()).unwrap(),
        bytes,
        "saved as it was read"
    );
}

#[test]
fn nel_stays_a_character_where_lines_break_otherwise() {
    // NEL and LF (0x25) in one file: both kept as they are.
    let mixed = [0xC1, 0x15, 0xC2, 0x25];
    let file = ebcdic(&mixed, "IBM037");
    assert_eq!(file.text.to_string(), "A\u{85}B\n");
    assert_ne!(file.format.line_ending, LineEnding::Nel);
    assert_eq!(
        crate::encode_document(&file.text, file.format).unwrap(),
        mixed
    );
    // No line break at all, and a code page that is not EBCDIC.
    assert_ne!(
        ebcdic(&[0xC1], "IBM037").format.line_ending,
        LineEnding::Nel
    );
    assert_ne!(ebcdic(&[], "IBM037").format.line_ending, LineEnding::Nel);
    assert_ne!(
        ebcdic(&[b'a', 0x15], "IBM437").format.line_ending,
        LineEnding::Nel
    );
}

#[test]
fn nel_is_written_only_in_ebcdic() {
    let text = Rope::from_str("a\nb");
    let nel = |encoding| Format {
        encoding,
        bom: false,
        line_ending: LineEnding::Nel,
    };
    assert_eq!(
        crate::encode_document(&text, nel(Encoding::Legacy("IBM875"))).unwrap(),
        [0x81, 0x15, 0x82]
    );
    assert_eq!(
        crate::encode_document(&text, nel(Encoding::Utf8)).unwrap(),
        b"a\nb",
        "after Convert to UTF-8, LF"
    );
    assert!(crate::is_ebcdic(Encoding::Legacy("IBM875")));
    assert!(crate::is_ebcdic(Encoding::Legacy("IBM1140")));
    assert!(!crate::is_ebcdic(Encoding::Legacy("IBM866")));
    assert!(!crate::is_ebcdic(Encoding::Legacy("windows-1253")));
    assert!(!crate::is_ebcdic(Encoding::Utf8));
}

#[test]
fn greek_ebcdic_reads_and_writes_greek() {
    // Alpha, omega and the final sigma in EBCDIC 875.
    let file = ebcdic(&[0x41, 0x67, 0xBA], "IBM875");
    assert_eq!(file.text.to_string(), "ΑΩς");
    assert!(file.problem.is_none());
    // 0xDC is SUB in the standard table too, but only 0x3F is kept as SUB.
    assert!(ebcdic(&[0xDC], "IBM875").problem.is_some());
    assert!(ebcdic(&[0x3F], "IBM875").problem.is_none());
}

#[test]
fn files_up_to_the_limit_are_read_and_larger_ones_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.bin");
    let progress = AtomicU64::new(0);

    // Empty, exactly at the limit, and one byte over it.
    fs::write(&path, b"").unwrap();
    let (bytes, info) = read_file_within(&path, &progress, 10).unwrap();
    assert!(bytes.is_empty());
    assert!(!info.read_only);
    fs::write(&path, b"0123456789").unwrap();
    let (bytes, _) = read_file_within(&path, &progress, 10).unwrap();
    assert_eq!(bytes, b"0123456789");
    assert_eq!(
        progress.load(Ordering::Relaxed),
        10,
        "progress counts every byte"
    );
    fs::write(&path, b"0123456789!").unwrap();
    let error = read_file_within(&path, &progress, 10).unwrap_err();
    assert!(
        matches!(
            error,
            ReadError::TooLarge {
                size: 11,
                limit: 10,
                ..
            }
        ),
        "{error}"
    );
}

#[test]
fn the_real_limit_is_two_gigabytes() {
    assert_eq!(MAX_FILE_SIZE, 1 << 31);
}
