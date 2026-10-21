use proptest::prelude::*;

use birchpad_core::Rope;
use birchpad_tools::hex::*;

#[test]
fn dumps_rows_of_sixteen_bytes() {
    let dumped = dump(b"Hello, world!\n", "\n");
    assert_eq!(
        dumped,
        "00000000  48 65 6C 6C 6F 2C 20 77  6F 72 6C 64 21 0A       |Hello, world!.|\n"
    );
    let two_rows = dump(&[0xFF; 17], "\r\n");
    let lines: Vec<&str> = two_rows.split("\r\n").collect();
    assert_eq!(lines.len(), 3);
    assert!(lines[0].starts_with("00000000  FF FF"));
    assert!(lines[0].ends_with("|................|"));
    assert_eq!(lines[1].len(), lines[0].len() - 15);
    assert!(lines[1].starts_with("00000010  FF "));
    assert_eq!(lines[2], "");
    // Every row's text column starts in the same place.
    assert_eq!(lines[0].find('|'), lines[1].find('|'));
}

#[test]
fn empty_dumps_to_nothing() {
    assert_eq!(dump(&[], "\n"), "");
    assert_eq!(parse("").unwrap(), b"");
    assert_eq!(parse("\n\n   \n").unwrap(), b"");
}

#[test]
fn reads_edited_rows() {
    let text = "00000000  41 42 |AB|\n\n00000002 43 44 45\n0000000a  0a|x|y|\n";
    assert_eq!(parse(text).unwrap(), b"ABCDE\n");
    // A row emptied of its bytes.
    assert_eq!(parse("00000000  |ab|\n00000002 41").unwrap(), b"A");
}

#[test]
fn says_which_line_does_not_read() {
    for (text, line) in [
        ("zz 41", 1),
        ("00 4", 1),
        ("00 414", 1),
        ("00 41\n10 G1", 2),
        ("00 +1", 1),
    ] {
        let error = parse(text).expect_err(text);
        assert_eq!(error.line, line, "{text}");
    }
    assert_eq!(
        parse("00 4").unwrap_err().to_string(),
        "line 1: \"4\" is not a byte in hexadecimal"
    );
}

proptest! {
    #[test]
    fn reads_back_what_it_dumps(bytes in proptest::collection::vec(any::<u8>(), 0..100)) {
        prop_assert_eq!(parse(&dump(&bytes, "\n")).unwrap(), bytes.clone());
        prop_assert_eq!(parse(&dump(&bytes, "\r\n")).unwrap(), bytes);
    }
}

#[test]
fn replaces_only_what_differs() {
    // The caret ends after what was put in, so the edit touches only that.
    for (old, new, caret) in [
        ("abc", "abc", 3),
        ("abc", "aXc", 2),
        ("", "new", 3),
        ("old", "", 0),
        ("мир", "миф", 6),
        ("aa", "aaa", 3),
        ("ab", "b", 0),
    ] {
        let rope = Rope::from(old);
        let transaction = replace_differing(&rope, new).unwrap();
        let mut applied = rope.clone();
        transaction.changes().apply(&mut applied);
        assert_eq!(applied.to_string(), new, "{old} -> {new}");
        assert_eq!(
            transaction.selection().unwrap().primary().head,
            caret,
            "{old} -> {new}"
        );
    }
}

#[test]
fn replacing_with_the_same_text_changes_nothing() {
    for text in ["", "a", "мир", "a\r\nb"] {
        let rope = Rope::from(text);
        let transaction = replace_differing(&rope, text).unwrap();
        assert!(transaction.changes().is_identity(), "{text:?}");
    }
}

#[test]
fn reads_patterns_of_bytes() {
    assert_eq!(parse_pattern("4A 6f20").unwrap(), [0x4A, 0x6F, 0x20]);
    assert_eq!(parse_pattern(" ff\t00\n").unwrap(), [0xFF, 0x00]);
    assert_eq!(parse_pattern("00").unwrap(), [0]);
    for wrong in ["", "   ", "4", "4A 6", "zz", "0x41", "4A-6F"] {
        assert!(parse_pattern(wrong).is_err(), "{wrong:?}");
    }
}

#[test]
fn finds_bytes_going_round() {
    let bytes = b"abcabc";
    assert_eq!(find(bytes, b"bc", 0, false), Some(1));
    assert_eq!(find(bytes, b"bc", 2, false), Some(4));
    // Past the last: round to the first.
    assert_eq!(find(bytes, b"bc", 5, false), Some(1));
    assert_eq!(find(bytes, b"bc", 99, false), Some(1));
    // Backward: the last that starts before, then round from the end.
    assert_eq!(find(bytes, b"bc", 4, true), Some(1));
    assert_eq!(find(bytes, b"bc", 1, true), Some(4));
    assert_eq!(find(bytes, b"abcabc", 0, false), Some(0));
    assert_eq!(find(bytes, b"abcabc", 3, true), Some(0));
    assert_eq!(find(bytes, b"x", 0, false), None);
    assert_eq!(find(bytes, b"", 0, false), None);
    assert_eq!(find(bytes, b"abcabca", 0, false), None);
    assert_eq!(find(b"", b"a", 0, true), None);
}

#[test]
fn shows_printable_bytes_as_themselves() {
    assert_eq!(printable(b'A'), 'A');
    assert_eq!(printable(b' '), ' ');
    assert_eq!(printable(b'~'), '~');
    for byte in [0u8, b'\n', 0x7F, 0x80, 0xFF] {
        assert_eq!(printable(byte), '.', "{byte}");
    }
}
