use birchpad_core::Rope;
use birchpad_view::*;

fn guides(text: &str, line: usize, headers: &[usize]) -> Vec<usize> {
    let text = Rope::from_str(text);
    indent_guides(&text, line, 4, |line| headers.contains(&line)).collect()
}

#[test]
fn indentation_in_columns() {
    let text = Rope::from_str("  \tx\n \t \n");
    assert_eq!(indentation(&text, 0, 4), (4, false));
    assert_eq!(indentation(&text, 1, 4), (5, true));
    assert_eq!(indentation(&text, 2, 4), (0, true), "the empty last line");
}

#[test]
fn guides_inside_the_indentation_only() {
    let text = "a\n    b\n        c\n\t\td\n      e";
    assert!(guides(text, 0, &[]).is_empty());
    assert!(guides(text, 1, &[]).is_empty(), "none at column 0");
    assert_eq!(guides(text, 2, &[]), [4]);
    assert_eq!(guides(text, 3, &[]), [4]);
    assert_eq!(guides(text, 4, &[]), [4], "a partial level");
}

#[test]
fn blank_lines_take_the_deeper_neighbor() {
    let text = "fn a() {\n        x\n\n    y\n}";
    assert_eq!(guides(text, 2, &[]), [4], "the line above is deeper");
    let text = "    if x {\n\n    }";
    assert!(guides(text, 1, &[]).is_empty());
    assert_eq!(
        guides(text, 1, &[0]),
        [4],
        "one level more below a fold header"
    );
    // Too far from any text.
    let text = format!("        x{}", "\n".repeat(30));
    assert!(guides(&text, 25, &[]).is_empty());
    assert_eq!(guides(&text, 5, &[]), [4]);
}
