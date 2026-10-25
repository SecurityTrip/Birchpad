use birchpad_core::Encoding;
use birchpad_core::Format;
use birchpad_core::LineEnding;
use birchpad_core::Rope;
use birchpad_io::*;
use std::fs;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

fn ebcdic(bytes: &[u8], name: &'static str) -> LoadedFile {
    let info = FileInfo {
        len: bytes.len() as u64,
        read_only: false,
        modified: None,
    };
    let options = LoadOptions {
        encoding: Some(Encoding::Legacy(name)),
        ansi: Encoding::Legacy("windows-1252"),
        guess: true,
        ascii_as: None,
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
        birchpad_io::encode_document(doc.text(), doc.format()).unwrap(),
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
        birchpad_io::encode_document(&file.text, file.format).unwrap(),
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
        birchpad_io::encode_document(&text, nel(Encoding::Legacy("IBM875"))).unwrap(),
        [0x81, 0x15, 0x82]
    );
    assert_eq!(
        birchpad_io::encode_document(&text, nel(Encoding::Utf8)).unwrap(),
        b"a\nb",
        "after Convert to UTF-8, LF"
    );
    assert!(birchpad_io::is_ebcdic(Encoding::Legacy("IBM875")));
    assert!(birchpad_io::is_ebcdic(Encoding::Legacy("IBM1140")));
    assert!(!birchpad_io::is_ebcdic(Encoding::Legacy("IBM866")));
    assert!(!birchpad_io::is_ebcdic(Encoding::Legacy("windows-1253")));
    assert!(!birchpad_io::is_ebcdic(Encoding::Utf8));
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

#[test]
fn ascii_files_take_the_encoding_asked_for_and_guessing_can_stop() {
    let decode = |bytes: &[u8], options: LoadOptions| {
        let info = FileInfo {
            len: bytes.len() as u64,
            read_only: false,
            modified: None,
        };
        decode_file(bytes.to_vec(), info, options).format
    };
    let ansi = Encoding::Legacy("windows-1252");
    let plain = LoadOptions::new(ansi);
    assert_eq!(decode(b"abc", plain).encoding, Encoding::Utf8);
    let with_bom = LoadOptions {
        ascii_as: Some((Encoding::Utf8, true)),
        ..plain
    };
    let format = decode(b"abc", with_bom);
    assert_eq!((format.encoding, format.bom), (Encoding::Utf8, true));
    // Not ASCII: as detected.
    assert!(!decode("é".as_bytes(), with_bom).bom);
    // A Unicode encoding other than UTF-8 would change the bytes: not taken.
    let utf16 = LoadOptions {
        ascii_as: Some((Encoding::Utf16Le, true)),
        ..plain
    };
    assert_eq!(decode(b"abc", utf16).encoding, Encoding::Utf8);
    // Without guessing, bytes that are not UTF-8 are ANSI.
    let cyrillic = [0xcf, 0xf0, 0xe8, 0xe2, 0xe5, 0xf2, 0x20, 0xec, 0xe8, 0xf0];
    let guessing = birchpad_io::detect(&cyrillic, ansi);
    let not_guessing = birchpad_io::detect_without_guess(&cyrillic, ansi);
    assert_eq!(not_guessing.encoding, ansi);
    assert_eq!(not_guessing.by, DetectedBy::Ansi);
    let _ = guessing;
    assert_eq!(
        decode(
            &cyrillic,
            LoadOptions {
                guess: false,
                ..plain
            }
        )
        .encoding,
        ansi
    );
    assert_eq!(
        birchpad_io::detect_without_guess(b"\xef\xbb\xbfa", ansi).by,
        DetectedBy::Bom
    );
}
