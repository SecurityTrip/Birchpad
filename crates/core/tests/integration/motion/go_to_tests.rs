use birchpad_core::motion::*;
use ropey::Rope;

#[test]
fn go_to_clamps_like_notepad_plus_plus() {
    let text = Rope::from_str("one\r\ntwo\nthree");
    assert_eq!(go_to_position(&text, GoToTarget::Line(2)), 5);
    assert_eq!(go_to_position(&text, GoToTarget::Line(99)), 9);
    assert_eq!(
        go_to_position(&text, GoToTarget::Offset(4)),
        3,
        "not inside CRLF"
    );
    assert_eq!(go_to_position(&text, GoToTarget::Offset(999)), text.len());
    let cyrillic = Rope::from_str("жж");
    assert_eq!(
        go_to_position(&cyrillic, GoToTarget::Offset(1)),
        0,
        "char boundary"
    );
    // Line 0 is the first line; an empty text has its start only.
    assert_eq!(go_to_position(&text, GoToTarget::Line(0)), 0);
    let empty = Rope::new();
    assert_eq!(go_to_position(&empty, GoToTarget::Line(5)), 0);
    assert_eq!(go_to_position(&empty, GoToTarget::Offset(5)), 0);
    // Just before a lone CR or at the LF of a CRLF.
    let cr = Rope::from_str("a\rb");
    assert_eq!(
        go_to_position(&cr, GoToTarget::Offset(2)),
        2,
        "a lone CR is a line break"
    );
}
