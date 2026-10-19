use super::*;
use crate::ops::test_support::run;

const RUST: CommentTokens = CommentTokens {
    line: Some("//"),
    block: Some(("/*", "*/")),
};
const CSS: CommentTokens = CommentTokens {
    line: None,
    block: Some(("/*", "*/")),
};

#[test]
fn toggles_line_comments_aligned_to_the_block() {
    let toggled = run("[  a\n    b\n\n  c]", |t, s| toggle_comment(t, s, RUST));
    assert_eq!(toggled, "[  // a\n  //   b\n\n  // c]");
    let back = run(&toggled, |t, s| toggle_comment(t, s, RUST));
    assert_eq!(back, "[  a\n    b\n\n  c]");
    // A partly commented block gets commented.
    assert_eq!(
        run("[// a\nb]", |t, s| toggle_comment(t, s, RUST)),
        "[// // a\n// b]"
    );
    assert_eq!(run("x|y", |t, s| toggle_comment(t, s, RUST)), "// x|y");
}

#[test]
fn add_and_remove_line_comments() {
    assert_eq!(
        run("[a\n// b]", |t, s| comment_lines(t, s, RUST)),
        "[// a\n// // b]"
    );
    assert_eq!(
        run("[//a\n// b\nc]", |t, s| uncomment_lines(t, s, RUST)),
        "[a\nb\nc]"
    );
    // The selection end stays before the text added at it.
    assert_eq!(
        run("[a\nb]", |t, s| toggle_comment(t, s, CSS)),
        "[/* a */\n/* b] */"
    );
    assert_eq!(run("[/* a */]", |t, s| toggle_comment(t, s, CSS)), "[a]");
}

#[test]
fn block_comments() {
    assert_eq!(
        run("x [y z] w", |t, s| block_comment(t, s, RUST)),
        "x [/* y z */] w"
    );
    assert_eq!(
        run("x [/* y */] w", |t, s| block_comment(t, s, RUST)),
        "x [y] w"
    );
    assert_eq!(
        run("  co|de", |t, s| block_comment(t, s, RUST)),
        "  /* code */|"
    );
}
