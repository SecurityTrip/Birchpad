use birchpad_core::Rope;
use birchpad_lsp::lsp_types::{Position, PositionEncodingKind, Range};
use birchpad_lsp::position::*;

use PositionEncoding::{Utf8, Utf16};

fn rope(text: &str) -> Rope {
    Rope::from_str(text)
}

#[test]
fn positions_count_utf16_units_or_bytes() {
    // "ё" is 2 bytes and 1 UTF-16 unit; "😀" 4 bytes and 2 units.
    let text = rope("aё😀b\nx");
    let b = "aё😀".len();
    assert_eq!(position(&text, b, Utf16), Position::new(0, 4));
    assert_eq!(position(&text, b, Utf8), Position::new(0, 7));
    assert_eq!(offset(&text, Position::new(0, 4), Utf16), b);
    assert_eq!(offset(&text, Position::new(0, 7), Utf8), b);
    // The second line.
    let x = text.len() - 1;
    assert_eq!(position(&text, x, Utf16), Position::new(1, 0));
    assert_eq!(offset(&text, Position::new(1, 0), Utf8), x);
    // Every offset at a character boundary comes back.
    for at in [0, 1, 3, b, b + 1, x, text.len()] {
        for encoding in [Utf8, Utf16] {
            assert_eq!(
                offset(&text, position(&text, at, encoding), encoding),
                at,
                "{at} {encoding:?}"
            );
        }
    }
}

#[test]
fn every_line_break_ends_a_line() {
    let text = rope("a\r\nb\rc\nd");
    for (line, at) in [(0, 0), (1, 3), (2, 5), (3, 7)] {
        assert_eq!(position(&text, at, Utf16), Position::new(line, 0));
        assert_eq!(offset(&text, Position::new(line, 0), Utf16), at);
    }
    // The end of a line is before its CRLF.
    assert_eq!(position(&text, 1, Utf16), Position::new(0, 1));
}

#[test]
fn positions_past_their_line_or_the_text_are_clamped() {
    let text = rope("ab\r\ncd");
    // Past the end of a line: its end, before the line break.
    assert_eq!(offset(&text, Position::new(0, 99), Utf16), 2);
    assert_eq!(offset(&text, Position::new(0, u32::MAX), Utf8), 2);
    // Past the last line: the end of the text.
    assert_eq!(offset(&text, Position::new(2, 0), Utf16), text.len());
    assert_eq!(offset(&text, Position::new(u32::MAX, 5), Utf16), text.len());
    // Inside a character: its start.
    let text = rope("😀x");
    assert_eq!(offset(&text, Position::new(0, 1), Utf16), 0);
    assert_eq!(offset(&text, Position::new(0, 2), Utf8), 0);
    assert_eq!(offset(&text, Position::new(0, 2), Utf16), 4);
    // An offset past the end or inside a character.
    assert_eq!(position(&text, 99, Utf16), Position::new(0, 3));
    assert_eq!(position(&text, 2, Utf8), Position::new(0, 0));
}

#[test]
fn the_empty_text_and_its_last_empty_line() {
    let empty = rope("");
    assert_eq!(position(&empty, 0, Utf16), Position::new(0, 0));
    assert_eq!(offset(&empty, Position::new(0, 0), Utf16), 0);
    assert_eq!(offset(&empty, Position::new(0, 5), Utf8), 0);
    let text = rope("a\n");
    assert_eq!(position(&text, 2, Utf16), Position::new(1, 0));
    assert_eq!(offset(&text, Position::new(1, 3), Utf16), 2);
}

#[test]
fn ranges_both_ways() {
    let text = rope("let ё = 1;\nok");
    let range = lsp_range(&text, 4..6, Utf16);
    assert_eq!(range, Range::new(Position::new(0, 4), Position::new(0, 5)));
    assert_eq!(byte_range(&text, range, Utf16), 4..6);
    // A range that ends before it starts is empty, at its start.
    let backwards = Range::new(Position::new(1, 1), Position::new(0, 0));
    let start = text.len() - 1;
    assert_eq!(byte_range(&text, backwards, Utf8), start..start);
}

#[test]
fn the_server_chooses_the_encoding() {
    assert_eq!(PositionEncoding::of_server(None), Utf16);
    assert_eq!(
        PositionEncoding::of_server(Some(&PositionEncodingKind::UTF8)),
        Utf8
    );
    assert_eq!(
        PositionEncoding::of_server(Some(&PositionEncodingKind::UTF16)),
        Utf16
    );
    // UTF-32 was not offered: the default.
    assert_eq!(
        PositionEncoding::of_server(Some(&PositionEncodingKind::UTF32)),
        Utf16
    );
    assert_eq!(
        PositionEncoding::offered(),
        [PositionEncodingKind::UTF8, PositionEncodingKind::UTF16]
    );
    assert_eq!(PositionEncoding::default(), Utf16);
}
