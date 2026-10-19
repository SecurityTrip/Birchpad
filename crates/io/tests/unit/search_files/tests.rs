use std::fs;

use birchpad_core::search::Query;

use super::*;

const ANSI: Encoding = Encoding::Legacy("windows-1252");

fn searcher(pattern: &str) -> Searcher {
    Searcher::new(&Query {
        pattern: pattern.to_owned(),
        ..Query::default()
    })
    .unwrap()
}

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    fs::read(path).unwrap()
}

/// `bytes` with each `from` replaced by `to`, as raw bytes.
fn replace_bytes(bytes: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut rest = bytes;
    while let Some(at) = rest.windows(from.len()).position(|window| window == from) {
        out.extend_from_slice(&rest[..at]);
        out.extend_from_slice(to);
        rest = &rest[at + from.len()..];
    }
    out.extend_from_slice(rest);
    out
}

fn encoded(text: &str, encoding: Encoding) -> Vec<u8> {
    crate::encode(&Rope::from_str(text), encoding, false).unwrap()
}

#[test]
fn binary_means_a_zero_byte_in_the_first_64_kilobytes() {
    assert!(!is_binary(&Rope::new()));
    assert!(!is_binary(&Rope::from_str("plain text\n")));
    assert!(is_binary(&Rope::from_str("a\0b")));
    // The last byte looked at, and the first one past it, across the rope's chunks.
    let mut text = "x".repeat(BINARY_SAMPLE + 10);
    text.replace_range(BINARY_SAMPLE - 1..BINARY_SAMPLE, "\0");
    assert!(is_binary(&Rope::from_str(&text)));
    let mut text = "x".repeat(BINARY_SAMPLE + 10);
    text.replace_range(BINARY_SAMPLE..BINARY_SAMPLE + 1, "\0");
    assert!(!is_binary(&Rope::from_str(&text)));
}

#[test]
fn replacing_in_a_file_keeps_its_encoding_bom_and_line_endings() {
    let dir = tempfile::tempdir().unwrap();
    let cases = [
        (
            "windows-1251-crlf.txt",
            "Съешь",
            "Ешь",
            Encoding::Legacy("windows-1251"),
        ),
        (
            "koi8-r-lf.txt",
            "булок",
            "пирогов",
            Encoding::Legacy("KOI8-R"),
        ),
        ("utf-16be-bom-cr.txt", "quick", "slow", Encoding::Utf16Be),
        ("utf-8-bom-crlf.txt", "café", "кафе", Encoding::Utf8),
    ];
    for (name, from, to, encoding) in cases {
        let original = fixture(name);
        let path = dir.path().join(name);
        fs::write(&path, &original).unwrap();
        let count = replace_in_file(&path, &searcher(from), to, ANSI, None).unwrap();
        assert_eq!(count, 1, "{name}");
        let expected = replace_bytes(&original, &encoded(from, encoding), &encoded(to, encoding));
        assert_eq!(fs::read(&path).unwrap(), expected, "{name}");
    }
}

#[test]
fn files_without_a_match_are_left_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, "nothing here").unwrap();
    // Read-only too: a file with nothing to replace is not "skipped".
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions.clone()).unwrap();
    assert_eq!(
        replace_in_file(&path, &searcher("needle"), "x", ANSI, None),
        Ok(0)
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "nothing here");
    #[expect(clippy::permissions_set_readonly_false, reason = "a temporary file")]
    permissions.set_readonly(false);
    fs::set_permissions(&path, permissions).unwrap();
}

#[test]
fn files_that_cannot_be_changed_exactly_are_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let write = |name: &str, bytes: &[u8]| {
        let path = dir.path().join(name);
        fs::write(&path, bytes).unwrap();
        path
    };
    let a = searcher("a");

    // Binary, a decoding problem, characters the encoding lacks.
    let binary = write("nul.txt", &fixture("nul-bytes.txt"));
    assert_eq!(
        replace_in_file(&binary, &a, "x", ANSI, None),
        Err(Skipped::Binary)
    );
    let surrogate = write("lone.txt", &fixture("utf-16le-lone-surrogate.txt"));
    assert_eq!(
        replace_in_file(&surrogate, &a, "x", ANSI, None),
        Err(Skipped::NotExact)
    );
    let western = write("cp1252.txt", b"caf\xe9 a");
    assert_eq!(
        replace_in_file(&western, &a, "Ж", ANSI, None),
        Err(Skipped::NotExact)
    );
    // Each is still what it was.
    assert_eq!(fs::read(&binary).unwrap(), fixture("nul-bytes.txt"));
    assert_eq!(fs::read(&western).unwrap(), b"caf\xe9 a");
    // In the same encoding the replacement fits.
    assert_eq!(replace_in_file(&western, &a, "b", ANSI, None), Ok(2));
    assert_eq!(fs::read(&western).unwrap(), b"cbf\xe9 b");

    let read_only = write("ro.txt", b"a");
    let mut permissions = fs::metadata(&read_only).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&read_only, permissions.clone()).unwrap();
    assert_eq!(
        replace_in_file(&read_only, &a, "x", ANSI, None),
        Err(Skipped::ReadOnly)
    );
    #[expect(clippy::permissions_set_readonly_false, reason = "a temporary file")]
    permissions.set_readonly(false);
    fs::set_permissions(&read_only, permissions).unwrap();

    let missing = dir.path().join("missing.txt");
    assert_eq!(
        replace_in_file(&missing, &a, "x", ANSI, None),
        Err(Skipped::Failed)
    );
}

#[test]
fn the_lines_with_matches_and_how_many() {
    let text = Rope::from_str("a needle\nnone\nneedle needle");
    let (lines, count) = search_text(&text, &searcher("needle")).unwrap();
    assert_eq!(count, 3);
    let found: Vec<(usize, usize)> = lines.iter().map(|hit| (hit.line, hit.matches)).collect();
    assert_eq!(found, [(0, 1), (2, 2)]);
    assert_eq!(search_text(&text, &searcher("absent")), None);
    assert_eq!(
        search_text(&Rope::new(), &searcher("a")),
        None,
        "an empty text"
    );
}

#[test]
fn reading_skips_binary_and_unreadable_files() {
    let dir = tempfile::tempdir().unwrap();
    let text = dir.path().join("a.txt");
    fs::write(&text, b"caf\xc3\xa9").unwrap();
    assert_eq!(
        read_for_search(&text, ANSI).unwrap().text.to_string(),
        "café"
    );
    let binary = dir.path().join("b.bin");
    fs::write(&binary, fixture("nul-bytes.txt")).unwrap();
    assert_eq!(read_for_search(&binary, ANSI).err(), Some(Skipped::Binary));
    assert_eq!(
        read_for_search(&dir.path().join("missing"), ANSI).err(),
        Some(Skipped::Failed)
    );
    assert_eq!(
        read_for_search(dir.path(), ANSI).err(),
        Some(Skipped::Failed),
        "a folder"
    );
}
