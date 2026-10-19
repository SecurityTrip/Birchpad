use super::*;
use crate::change::ChangeSet;

fn convert(text: &str, target: LineEnding) -> String {
    let mut rope = Rope::from_str(text);
    let changes = ChangeSet::from_edits(&rope, convert_line_breaks(&rope, target)).unwrap();
    changes.apply(&mut rope);
    rope.to_string()
}

#[test]
fn converts_every_kind_of_break() {
    let mixed = "a\r\nb\nc\rd\r\r\ne\r";
    assert_eq!(convert(mixed, LineEnding::Lf), "a\nb\nc\nd\n\ne\n");
    assert_eq!(
        convert(mixed, LineEnding::CrLf),
        "a\r\nb\r\nc\r\nd\r\n\r\ne\r\n"
    );
    assert_eq!(convert(mixed, LineEnding::Cr), "a\rb\rc\rd\r\re\r");
    assert!(convert_line_breaks(&Rope::from_str("x\ny\n"), LineEnding::Lf).is_empty());
}

#[test]
fn nel_is_lf_in_the_text() {
    assert_eq!(convert("a\r\nb\rc", LineEnding::Nel), "a\nb\nc");
    assert!(convert_line_breaks(&Rope::from_str("x\ny\n"), LineEnding::Nel).is_empty());
    assert!(convert_line_breaks(&Rope::from_str(""), LineEnding::Nel).is_empty());
    assert_eq!(LineEnding::Nel.as_str(), "\n");
    assert_eq!(LineEnding::Nel.label(), "NEL");
    assert_eq!(
        LineEnding::detect(&Rope::from_str("a\u{85}b")),
        None,
        "a NEL character in the text is no line break"
    );
}

#[test]
fn line_break_at_a_position() {
    let text = Rope::from_str("a\r\nb\nc\rd");
    assert_eq!(LineEnding::at(&text, 0), None);
    assert_eq!(LineEnding::at(&text, 1), Some(LineEnding::CrLf));
    assert_eq!(LineEnding::at(&text, 4), Some(LineEnding::Lf));
    assert_eq!(LineEnding::at(&text, 6), Some(LineEnding::Cr));
    assert_eq!(LineEnding::at(&text, 8), None, "end of text");
    assert_eq!(LineEnding::CrLf.label(), "CRLF");
}

#[test]
fn crlf_split_across_chunks_is_one_break() {
    // Long enough for ropey to split it into several chunks at arbitrary places.
    let text = "abc\r\n".repeat(5000);
    let converted = convert(&text, LineEnding::Lf);
    assert_eq!(converted, "abc\n".repeat(5000));
}

#[test]
fn detects_first_line_break() {
    let detect = |s: &str| LineEnding::detect(&Rope::from_str(s));
    assert_eq!(detect("a\r\nb\nc"), Some(LineEnding::CrLf));
    assert_eq!(detect("a\nb\r\nc"), Some(LineEnding::Lf));
    assert_eq!(detect("a\rb"), Some(LineEnding::Cr));
    assert_eq!(detect("trailing\r"), Some(LineEnding::Cr));
    assert_eq!(detect("one line"), None);
}
