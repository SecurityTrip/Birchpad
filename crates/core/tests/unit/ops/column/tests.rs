use super::*;

fn numbers(sequence: NumberSequence, rows: usize) -> Vec<String> {
    sequence.generate(rows)
}

#[test]
fn numbers_are_padded_to_the_widest() {
    let plain = NumberSequence::default();
    assert_eq!(numbers(plain, 3), ["1", "2", "3"]);
    let ten = numbers(plain, 10);
    assert_eq!(ten[0], "1 ", "left-aligned with trailing spaces");
    assert_eq!(ten[9], "10");
    let zeros = NumberSequence {
        leading: Leading::Zeros,
        initial: 8,
        ..plain
    };
    assert_eq!(numbers(zeros, 3), ["08", "09", "10"]);
    let spaces = NumberSequence {
        leading: Leading::Spaces,
        initial: 9,
        ..plain
    };
    assert_eq!(numbers(spaces, 2), [" 9", "10"]);
}

#[test]
fn step_repeat_and_negative_numbers() {
    let sequence = NumberSequence {
        initial: 2,
        step: -3,
        repeat: 2,
        leading: Leading::Zeros,
        ..NumberSequence::default()
    };
    assert_eq!(numbers(sequence, 6), ["02", "02", "-1", "-1", "-4", "-4"]);
    let constant = NumberSequence {
        step: 0,
        initial: 7,
        ..NumberSequence::default()
    };
    assert_eq!(numbers(constant, 2), ["7", "7"]);
}

#[test]
fn other_bases() {
    let hex = NumberSequence {
        initial: 1,
        step: 11,
        base: Base::Hex,
        ..NumberSequence::default()
    };
    assert_eq!(numbers(hex, 5), ["1 ", "C ", "17", "22", "2D"]);
    let lower = NumberSequence {
        uppercase: false,
        ..hex
    };
    assert_eq!(numbers(lower, 5)[4], "2d");
    let binary = NumberSequence {
        base: Base::Bin,
        leading: Leading::Zeros,
        initial: 0,
        ..NumberSequence::default()
    };
    assert_eq!(numbers(binary, 4), ["00", "01", "10", "11"]);
    assert_eq!(format_number(8, Base::Oct, true), "10");
    assert_eq!(parse_number("ff", Base::Hex), Some(255));
    assert_eq!(parse_number(" -12 ", Base::Dec), Some(-12));
    assert_eq!(parse_number("12", Base::Bin), None);
    assert_eq!(parse_number("", Base::Dec), None);
}

#[test]
fn rows_go_into_targets_and_past_the_end() {
    let text = Rope::from_str("abc\nx\nlast");
    // Column 2: inside "abc", virtual space after "x"; then two more lines than exist.
    let targets = [Range::point(2), Range::virtual_point(5, 1), Range::point(8)];
    let rows: Vec<String> = ["1", "2", "3", "4"].map(String::from).to_vec();
    let transaction = insert_rows(&text, &targets, &rows, 2, LineEnding::Lf);
    let mut result = text.clone();
    transaction.changes().apply(&mut result);
    assert_eq!(result, "ab1c\nx 2\nla3st\n  4");
    let carets: Vec<usize> = transaction
        .selection()
        .unwrap()
        .iter()
        .map(|range| range.head)
        .collect();
    assert_eq!(carets, [3, 8, 12, 18]);

    // A wider rectangle is replaced; extra target rows are emptied.
    let targets = [Range::new(0, 2), Range::new(4, 5)];
    let transaction = insert_rows(&text, &targets, &["Z".to_owned()], 0, LineEnding::Lf);
    let mut result = text.clone();
    transaction.changes().apply(&mut result);
    assert_eq!(result, "Zc\n\nlast");
}
