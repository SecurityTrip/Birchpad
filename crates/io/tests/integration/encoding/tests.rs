use birchpad_core::{Encoding, Rope};
use birchpad_io::*;

#[test]
fn every_character_set_is_known() {
    for (_, sets) in CHARACTER_SETS {
        for (name, _) in *sets {
            // The canonical name, which reads back as itself.
            assert_eq!(
                parse_encoding(name, Encoding::Utf8),
                Some((Encoding::Legacy(name), false)),
                "{name}"
            );
            let encoding = Encoding::Legacy(name);
            assert!(!encoding.is_unicode());
            let text = decode(b"Birchpad 1".to_vec(), encoding);
            if is_ebcdic(encoding) {
                continue;
            }
            assert_eq!(text.text, "Birchpad 1", "{name} is ASCII-compatible");
            assert_eq!(
                encode(&Rope::from_str("Birchpad 1"), encoding, false).unwrap(),
                b"Birchpad 1",
                "{name}"
            );
        }
    }
    // Birchpad's own code pages by their numbers.
    for (number, name) in [
        (437, "IBM437"),
        (737, "IBM737"),
        (1140, "IBM1140"),
        (875, "IBM875"),
    ] {
        assert_eq!(from_code_page(number), Some(Encoding::Legacy(name)));
        assert_eq!(
            parse_encoding(&format!("cp{number}"), Encoding::Utf8),
            Some((Encoding::Legacy(name), false))
        );
    }
}

#[test]
fn parses_command_names() {
    let ansi = Encoding::Legacy("windows-1251");
    assert_eq!(
        parse_encoding("utf-8-bom", ansi),
        Some((Encoding::Utf8, true))
    );
    assert_eq!(
        parse_encoding("UTF-16LE-BOM", ansi),
        Some((Encoding::Utf16Le, true))
    );
    assert_eq!(parse_encoding("ansi", ansi), Some((ansi, false)));
    assert_eq!(
        parse_encoding("cp866", ansi),
        Some((Encoding::Legacy("IBM866"), false))
    );
    assert_eq!(
        parse_encoding("koi8-r", ansi),
        Some((Encoding::Legacy("KOI8-R"), false))
    );
    assert_eq!(parse_encoding("replacement", ansi), None);
    assert_eq!(parse_encoding("klingon", ansi), None);
    for (encoding, bom) in [
        (Encoding::Utf8, false),
        (Encoding::Utf8, true),
        (Encoding::Utf16Be, true),
        (Encoding::Legacy("Shift_JIS"), false),
    ] {
        assert_eq!(
            parse_encoding(&encoding_name(encoding, bom), ansi),
            Some((encoding, bom))
        );
    }
}

#[test]
fn display_names_follow_notepad_plus_plus() {
    assert_eq!(display_name(Encoding::Utf8, true), "UTF-8-BOM");
    assert_eq!(display_name(Encoding::Utf16Le, true), "UTF-16 LE BOM");
    assert_eq!(display_name(Encoding::Legacy("IBM866"), false), "OEM 866");
    assert_eq!(
        display_name(Encoding::Legacy("windows-1251"), false),
        "Windows-1251"
    );
}

#[test]
fn maps_windows_code_pages() {
    assert_eq!(from_code_page(1251), Some(Encoding::Legacy("windows-1251")));
    assert_eq!(from_code_page(866), Some(Encoding::Legacy("IBM866")));
    assert_eq!(from_code_page(932), Some(Encoding::Legacy("Shift_JIS")));
    assert_eq!(from_code_page(28595), Some(Encoding::Legacy("ISO-8859-5")));
    assert_eq!(from_code_page(65001), None);
    assert!(!system_ansi().is_unicode());
}

#[test]
fn code_pages_at_the_ends_of_their_ranges() {
    // Windows-1250 to 1258, and nothing either side.
    assert_eq!(from_code_page(1250), Some(Encoding::Legacy("windows-1250")));
    assert_eq!(from_code_page(1258), Some(Encoding::Legacy("windows-1258")));
    assert_eq!(from_code_page(1249), None);
    assert_eq!(from_code_page(1259), None);
    // ISO 8859-2 to 16; part 12 was never published, and part 1 is Windows-1252 in WHATWG.
    assert_eq!(from_code_page(28592), Some(Encoding::Legacy("ISO-8859-2")));
    assert_eq!(from_code_page(28606), Some(Encoding::Legacy("ISO-8859-16")));
    assert_eq!(from_code_page(28602), None);
    assert_eq!(from_code_page(28591), None);
    assert_eq!(from_code_page(28607), None);
    // The others one by one, and numbers that are no code page.
    for (code_page, name) in [
        (874, "windows-874"),
        (936, "GBK"),
        (949, "EUC-KR"),
        (950, "Big5"),
        (10000, "macintosh"),
        (10007, "x-mac-cyrillic"),
        (20866, "KOI8-R"),
        (21866, "KOI8-U"),
        (54936, "gb18030"),
        // DOS and EBCDIC code pages (Birchpad's own tables).
        (437, "IBM437"),
        (37, "IBM037"),
        (1140, "IBM1140"),
    ] {
        assert_eq!(
            from_code_page(code_page),
            Some(Encoding::Legacy(name)),
            "{code_page}"
        );
    }
    for nothing in [0, 1, 436, 65000, u32::MAX] {
        assert_eq!(from_code_page(nothing), None, "{nothing}");
    }
}

#[test]
fn command_names_have_aliases_and_ignore_case_and_spaces() {
    let ansi = Encoding::Legacy("windows-1252");
    for (name, expected) in [
        ("utf8", (Encoding::Utf8, false)),
        ("utf8-bom", (Encoding::Utf8, true)),
        (" UTF-8 ", (Encoding::Utf8, false)),
        ("utf-16", (Encoding::Utf16Le, false)),
        ("utf-16-bom", (Encoding::Utf16Le, true)),
        ("utf-16le", (Encoding::Utf16Le, false)),
        ("utf-16be", (Encoding::Utf16Be, false)),
        ("ANSI", (ansi, false)),
        ("Windows-1251", (Encoding::Legacy("windows-1251"), false)),
    ] {
        assert_eq!(parse_encoding(name, ansi), Some(expected), "{name:?}");
    }
    // Nothing, a BOM on what has none, and encodings Birchpad cannot save.
    for name in ["", "  ", "ansi-bom", "windows-1251-bom", "x-user-defined"] {
        assert_eq!(parse_encoding(name, ansi), None, "{name:?}");
    }
}

#[test]
fn every_unicode_form_has_its_display_name() {
    let names = [
        ((Encoding::Utf8, false), "UTF-8"),
        ((Encoding::Utf8, true), "UTF-8-BOM"),
        ((Encoding::Utf16Le, false), "UTF-16 LE"),
        ((Encoding::Utf16Le, true), "UTF-16 LE BOM"),
        ((Encoding::Utf16Be, false), "UTF-16 BE"),
        ((Encoding::Utf16Be, true), "UTF-16 BE BOM"),
    ];
    for ((encoding, bom), name) in names {
        assert_eq!(display_name(encoding, bom), name);
        assert_eq!(
            parse_encoding(&encoding_name(encoding, bom), encoding),
            Some((encoding, bom))
        );
    }
    // A legacy encoding has no BOM to show, and one outside the menus keeps its name.
    assert_eq!(
        display_name(Encoding::Legacy("windows-1251"), true),
        "Windows-1251"
    );
    assert_eq!(display_name(Encoding::Legacy("no-such"), false), "no-such");
}

#[test]
fn encodings_without_text_are_refused() {
    // encoding_rs's replacement and x-user-defined are no encodings of text files.
    for label in ["replacement", "x-user-defined", "iso-2022-kr"] {
        assert_eq!(parse_encoding(label, Encoding::Utf8), None, "{label}");
    }
    for (label, encoding) in [
        ("utf-8", Encoding::Utf8),
        ("utf-16le", Encoding::Utf16Le),
        ("utf-16be", Encoding::Utf16Be),
        ("windows-1251", Encoding::Legacy("windows-1251")),
    ] {
        assert_eq!(
            parse_encoding(label, Encoding::Utf8),
            Some((encoding, false))
        );
        assert_eq!(
            parse_encoding(&encoding_name(encoding, false), Encoding::Utf8),
            Some((encoding, false))
        );
    }
}

#[test]
#[should_panic(expected = "unknown legacy encoding klingon")]
fn an_unknown_legacy_encoding_is_a_bug() {
    decode(b"a".to_vec(), Encoding::Legacy("klingon"));
}
