use super::*;

#[test]
fn line_and_column_count_characters() {
    let text = "ab\nмир x";
    assert_eq!(SyntaxError::new(0, "").line_column(text), (1, 1));
    assert_eq!(SyntaxError::new(2, "").line_column(text), (1, 3));
    assert_eq!(SyntaxError::new(3, "").line_column(text), (2, 1));
    // After three two-byte letters and a space.
    assert_eq!(SyntaxError::new(10, "").line_column(text), (2, 5));
    // Inside a character, and past the end.
    assert_eq!(SyntaxError::new(4, "").line_column(text), (2, 1));
    assert_eq!(SyntaxError::new(99, "").line_column(text), (2, 6));
    assert_eq!(SyntaxError::new(0, "").line_column(""), (1, 1));
}

#[test]
fn indents_by_tabs_or_spaces() {
    let mut out = String::new();
    Indent::Tab.write(&mut out, 2);
    Indent::Spaces(3).write(&mut out, 1);
    Indent::Spaces(0).write(&mut out, 5);
    Indent::Spaces(2).write(&mut out, 0);
    assert_eq!(out, "\t\t   ");
    assert_eq!(SyntaxError::new(1, "bad").to_string(), "bad");
}
