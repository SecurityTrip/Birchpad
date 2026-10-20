use crate::ops_test_support::run;
use birchpad_core::ops::*;

#[test]
fn indent_the_selected_lines() {
    assert_eq!(
        run("[a\nb\n]c", |t, s| indent_lines(t, s, "\t")),
        "[\ta\n\tb\n]c",
        "the line the selection ends at the start of stays"
    );
    assert_eq!(run("a|b", |t, s| indent_lines(t, s, "    ")), "    a|b");
    assert_eq!(run("|ab", |t, s| indent_lines(t, s, "  ")), "  |ab");
    // Empty lines stay empty; two carets on one line indent it once.
    assert_eq!(
        run("[a\n\nb]", |t, s| indent_lines(t, s, "\t")),
        "[\ta\n\n\tb]"
    );
    assert_eq!(run("a|b|c", |t, s| indent_lines(t, s, "\t")), "\ta|b|c");
    // CRLF, wide characters, the last line without a break.
    assert_eq!(
        run("[é\r\n日本]", |t, s| indent_lines(t, s, "\t")),
        "[\té\r\n\t日本]"
    );
}

#[test]
fn indent_with_nothing_to_do() {
    assert_eq!(run("|", |t, s| indent_lines(t, s, "\t")), "|");
    assert_eq!(run("a|", |t, s| indent_lines(t, s, "")), "a|");
}

#[test]
fn outdent_by_a_tab_or_to_the_previous_stop() {
    assert_eq!(run("\ta|", |t, s| outdent_lines(t, s, 4)), "a|");
    assert_eq!(run("        a|", |t, s| outdent_lines(t, s, 4)), "    a|");
    assert_eq!(run("      a|", |t, s| outdent_lines(t, s, 4)), "    a|");
    assert_eq!(run("  a|", |t, s| outdent_lines(t, s, 4)), "a|");
    assert_eq!(run(" \ta|", |t, s| outdent_lines(t, s, 4)), "\ta|");
    assert_eq!(
        run("[\ta\nb\n    c]", |t, s| outdent_lines(t, s, 4)),
        "[a\nb\nc]"
    );
    // A caret inside the indentation goes to the line's start.
    assert_eq!(run("  |  a", |t, s| outdent_lines(t, s, 4)), "|a");
}

#[test]
fn outdent_with_nothing_to_do() {
    assert_eq!(run("a|\nb", |t, s| outdent_lines(t, s, 4)), "a|\nb");
    assert_eq!(run("|", |t, s| outdent_lines(t, s, 4)), "|");
    // A tab width of zero counts as one.
    assert_eq!(run("  a|", |t, s| outdent_lines(t, s, 0)), " a|");
}
