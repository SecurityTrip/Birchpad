use crate::ops_test_support::run;
use birchpad_core::ops::*;
use birchpad_core::*;

const LF: LineEnding = LineEnding::Lf;

#[test]
fn duplicate_lines_and_selections() {
    assert_eq!(run("a|b\ncd", |t, s| duplicate(t, s, LF)), "a|b\nab\ncd");
    assert_eq!(run("ab\nc|d", |t, s| duplicate(t, s, LF)), "ab\nc|d\ncd");
    assert_eq!(run("[ab]c", |t, s| duplicate(t, s, LF)), "[ab]abc");
    // Two carets on one line duplicate it once.
    assert_eq!(run("|a|b", |t, s| duplicate(t, s, LF)), "|a|b\nab");
}

#[test]
fn delete_lines_including_the_last() {
    assert_eq!(run("a\nb|b\nc", delete_lines), "a\n|c");
    assert_eq!(run("a\nb\nc|c", delete_lines), "a\nb|");
    assert_eq!(run("[a\nb\n]c", delete_lines), "|c");
    assert_eq!(run("on|ly", delete_lines), "|");
}

#[test]
fn move_lines_carries_selections() {
    assert_eq!(
        run("a\nb|b\nc", |t, s| move_lines(t, s, true, LF)),
        "b|b\na\nc"
    );
    assert_eq!(
        run("a\nb|b\nc", |t, s| move_lines(t, s, false, LF)),
        "a\nc\nb|b"
    );
    assert_eq!(
        run("a\n[b\nc]\nd", |t, s| move_lines(t, s, true, LF)),
        "[b\nc]\na\nd"
    );
    // Whole lines selected down to the start of the next one, as with the mouse.
    assert_eq!(
        run("a\n[b\n]c", |t, s| move_lines(t, s, true, LF)),
        "[b\n]a\nc"
    );
    assert_eq!(
        run("[a\n]b\nc", |t, s| move_lines(t, s, false, LF)),
        "b\n[a\n]c"
    );
    // Mixed line breaks inside a moved block become the document's.
    assert_eq!(
        run("x\n|a\r\nb", |t, s| move_lines(t, s, true, LF)),
        "|a\nx\r\nb"
    );
    assert_eq!(
        run("|a\nb", |t, s| move_lines(t, s, true, LF)),
        "|a\nb",
        "at the top"
    );
    // Two separate blocks move together.
    assert_eq!(
        run("a\n|b\nc\n|d\ne", |t, s| move_lines(t, s, false, LF)),
        "a\nc\n|b\ne\n|d"
    );
}

#[test]
fn blank_lines_above_and_below() {
    assert_eq!(
        run("a\nb|c", |t, s| insert_blank_line(t, s, true, LF)),
        "a\n|\nbc"
    );
    assert_eq!(
        run("a|b\nc", |t, s| insert_blank_line(t, s, false, LF)),
        "ab\n|\nc"
    );
}

#[test]
fn join_and_split() {
    assert_eq!(
        run("a|b\n   cd\nef", |t, s| join_lines(t, s, LF)),
        "|ab cd\nef"
    );
    assert_eq!(
        run("[one\n  two\n\nthree]", |t, s| join_lines(t, s, LF)),
        "[one two three]"
    );
    assert_eq!(
        run("the quick| brown fox", |t, s| split_lines(t, s, 9, LF)),
        "|the quick\nbrown fox"
    );
}
