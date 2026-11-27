use birchpad_tools::csv::{MAX_BYTES, MAX_COLUMNS, MAX_ROWS, TableError, parse};

#[test]
fn reads_quoted_fields_and_keeps_the_first_record() {
    let rows = parse("name,note\r\n望,\"a,b\r\n\"\"c\"\"\"\r\nlast,").unwrap();
    assert_eq!(&rows[0], &vec!["name", "note"]);
    assert_eq!(&rows[1], &vec!["望", "a,b\r\n\"c\""]);
    assert_eq!(&rows[2], &vec!["last", ""]);
}

#[test]
fn reports_unequal_width_with_a_record_position() {
    let Err(TableError::Invalid(error)) = parse("a,b\nx") else {
        panic!("expected an unequal record length");
    };
    assert_eq!(error.position().unwrap().line(), 2);
}

#[test]
fn accepts_empty_input_and_enforces_each_limit() {
    assert!(parse("").unwrap().is_empty());
    assert_eq!(&parse("\"\"").unwrap()[0], &vec![""]);
    let bytes = "a".repeat(MAX_BYTES);
    assert!(parse(&bytes).is_ok());
    assert!(matches!(
        parse(&(bytes + "a")),
        Err(TableError::TooManyBytes)
    ));
    let columns = ",".repeat(MAX_COLUMNS - 1);
    assert_eq!(parse(&columns).unwrap()[0].len(), MAX_COLUMNS);
    assert!(matches!(
        parse(&(columns + ",")),
        Err(TableError::TooManyColumns)
    ));
    let rows = "a\n".repeat(MAX_ROWS);
    assert_eq!(parse(&rows).unwrap().len(), MAX_ROWS);
    assert!(matches!(parse(&(rows + "a")), Err(TableError::TooManyRows)));
}
