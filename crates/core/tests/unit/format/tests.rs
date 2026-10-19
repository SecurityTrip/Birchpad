use super::*;

#[test]
fn unicode_encodings_are_told_from_legacy_ones() {
    for encoding in [Encoding::Utf8, Encoding::Utf16Le, Encoding::Utf16Be] {
        assert!(encoding.is_unicode(), "{encoding}");
    }
    for name in ["windows-1252", "Shift_JIS", ""] {
        assert!(!Encoding::Legacy(name).is_unicode(), "{name:?}");
    }
}

#[test]
fn encodings_show_their_names() {
    assert_eq!(Encoding::Utf8.to_string(), "UTF-8");
    assert_eq!(Encoding::Utf16Le.to_string(), "UTF-16 LE");
    assert_eq!(Encoding::Utf16Be.to_string(), "UTF-16 BE");
    assert_eq!(Encoding::Legacy("KOI8-R").to_string(), "KOI8-R");
    // A legacy encoding is shown by its name as it is, even an empty one.
    assert_eq!(Encoding::Legacy("").to_string(), "");
}

#[test]
fn a_new_document_is_utf8_without_bom_with_the_native_line_ending() {
    let format = Format::default();
    assert_eq!(format, Format::new());
    assert_eq!(format.encoding, Encoding::Utf8);
    assert!(!format.bom);
    assert_eq!(format.line_ending, LineEnding::native());
}

#[test]
fn with_line_ending_changes_only_the_line_ending() {
    let utf16 = Format {
        encoding: Encoding::Utf16Be,
        bom: true,
        line_ending: LineEnding::Lf,
    };
    for line_ending in [LineEnding::CrLf, LineEnding::Lf, LineEnding::Cr] {
        let changed = utf16.with_line_ending(line_ending);
        assert_eq!(changed.line_ending, line_ending);
        assert_eq!((changed.encoding, changed.bom), (Encoding::Utf16Be, true));
    }
}
