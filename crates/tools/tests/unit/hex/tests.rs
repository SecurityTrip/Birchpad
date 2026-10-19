use proptest::prelude::*;

use super::*;

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
