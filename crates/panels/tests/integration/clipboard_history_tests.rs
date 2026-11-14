use birchpad_panels::clipboard_history::*;
use std::collections::VecDeque;

#[test]
fn the_newest_text_comes_first_once() {
    let mut history = VecDeque::new();
    assert!(remember(&mut history, "a"));
    assert!(remember(&mut history, "b"));
    assert!(!remember(&mut history, "b"), "the same as the top");
    assert!(remember(&mut history, "a"), "moves up");
    assert_eq!(history, ["a", "b"]);
    // Empty and huge texts are not kept.
    assert!(!remember(&mut history, ""));
    assert!(!remember(&mut history, &"x".repeat(MAX_TEXT + 1)));
    assert!(
        remember(&mut history, &"x".repeat(MAX_TEXT)),
        "exactly the limit"
    );
    // The oldest go past the limit.
    for n in 0..LIMIT + 5 {
        remember(&mut history, &n.to_string());
    }
    assert_eq!(history.len(), LIMIT);
    assert_eq!(history.front().map(String::as_str), Some("34"));
    assert_eq!(history.back().map(String::as_str), Some("5"));
}

#[test]
fn previews_are_one_line_and_short() {
    assert_eq!(preview("one\r\ntwo\nthree\tfour"), "one⏎two⏎three four");
    assert_eq!(preview(""), "");
    let long = "ж".repeat(PREVIEW);
    assert_eq!(preview(&long), long, "exactly the length shown");
    let longer = "ж".repeat(PREVIEW + 1);
    assert_eq!(preview(&longer), format!("{long}…"));
}
