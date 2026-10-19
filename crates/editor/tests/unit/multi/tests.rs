use super::*;

#[test]
fn clipboard_rows_split_at_any_line_break() {
    assert_eq!(split_lines("a\r\nb\nc\rd"), ["a", "b", "c", "d"]);
    let item = ClipboardItem::new_string_with_json_metadata(
        "ab\r\ncd\r\n".to_owned(),
        RectangleClipboard {
            rectangular: true,
            rows: 2,
        },
    );
    assert_eq!(
        rectangle_rows(&item, "ab\r\ncd\r\n"),
        Some(vec!["ab".to_owned(), "cd".to_owned()])
    );
    assert_eq!(rectangle_rows(&item, "other text\n"), None, "stale marker");
    let plain = ClipboardItem::new_string("ab\ncd\n".to_owned());
    assert_eq!(rectangle_rows(&plain, "ab\ncd\n"), None);
}

#[test]
fn column_insert_arguments() {
    let text: ColumnInsertArgs = serde_json::from_str(r#"{ "text": "x" }"#).unwrap();
    assert_eq!(text, ColumnInsertArgs::Text { text: "x".into() });
    let numbers: ColumnInsertArgs =
        serde_json::from_str(r#"{ "initial": 5, "format": "hex", "leading": "zeros" }"#).unwrap();
    let ColumnInsertArgs::Numbers(numbers) = numbers else {
        panic!("numbers expected");
    };
    assert_eq!(
        (
            numbers.initial,
            numbers.step,
            numbers.repeat,
            numbers.format
        ),
        (5, 1, 1, FormatName::Hex)
    );
    assert_eq!(numbers.leading, LeadingName::Zeros);
}
