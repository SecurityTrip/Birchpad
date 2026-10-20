use birchpad_tools::*;

fn error(offset: usize, message: &str) -> SyntaxError {
    SyntaxError {
        offset,
        message: message.to_owned(),
    }
}

#[test]
fn line_and_column_count_characters() {
    let text = "ab\nмир x";
    assert_eq!(error(0, "").line_column(text), (1, 1));
    assert_eq!(error(2, "").line_column(text), (1, 3));
    assert_eq!(error(3, "").line_column(text), (2, 1));
    // After three two-byte letters and a space.
    assert_eq!(error(10, "").line_column(text), (2, 5));
    // Inside a character, and past the end.
    assert_eq!(error(4, "").line_column(text), (2, 1));
    assert_eq!(error(99, "").line_column(text), (2, 6));
    assert_eq!(error(0, "").line_column(""), (1, 1));
}

#[test]
fn indents_by_tabs_or_spaces() {
    let formatted = |indent| {
        let layout = Layout {
            indent,
            line_ending: "\n",
        };
        json::format(r#"{"a":{"b":1}}"#, layout).unwrap()
    };
    assert_eq!(
        formatted(Indent::Tab),
        "{\n\t\"a\": {\n\t\t\"b\": 1\n\t}\n}"
    );
    assert_eq!(
        formatted(Indent::Spaces(3)),
        "{\n   \"a\": {\n      \"b\": 1\n   }\n}"
    );
    // No indentation: every level at the start of the line.
    assert_eq!(formatted(Indent::Spaces(0)), "{\n\"a\": {\n\"b\": 1\n}\n}");
    assert_eq!(error(1, "bad").to_string(), "bad");
}
