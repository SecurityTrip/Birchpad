use crate::ops_test_support::run;
use birchpad_core::ops::*;
use birchpad_core::*;

const LF: LineEnding = LineEnding::Lf;

#[test]
fn trims() {
    assert_eq!(
        run("  a  \n\tb\t", |t, s| trim(t, s, Trim::Trailing, LF)),
        "|  a\n\tb"
    );
    assert_eq!(
        run("  a  \n\tb\t", |t, s| trim(t, s, Trim::Leading, LF)),
        "|a  \nb\t"
    );
    assert_eq!(
        run("  a  \n[\tb\t]", |t, s| trim(t, s, Trim::Both, LF)),
        "  a  \n[b]"
    );
}

#[test]
fn eol_to_space_joins() {
    assert_eq!(
        run("a\n b \n\nc", |t, s| eol_to_space(t, s, false, LF)),
        "|a  b   c"
    );
    assert_eq!(
        run("a\n b \n\nc", |t, s| eol_to_space(t, s, true, LF)),
        "|a b c"
    );
}

#[test]
fn tabs_and_spaces() {
    assert_eq!(
        run("\tx\ta\tb", |t, s| tabs_to_spaces(t, s, 4, LF)),
        "|    x   a   b"
    );
    assert_eq!(
        run("        x   y    z", |t, s| spaces_to_tabs(
            t, s, 4, false, LF
        )),
        "|\t\tx\ty\t z"
    );
    assert_eq!(
        run("        x       y", |t, s| spaces_to_tabs(
            t, s, 4, true, LF
        )),
        "|\t\tx       y"
    );
}
