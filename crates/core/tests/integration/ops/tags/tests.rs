use birchpad_core::Range as Span;
use birchpad_core::ops::*;
use birchpad_core::*;

fn close(text: &str, carets: &[usize], html: bool) -> Option<(String, Vec<usize>)> {
    let mut rope = Rope::from_str(text);
    let selection = Selection::new(carets.iter().map(|&at| Span::point(at)), 0);
    let transaction = close_tags(&rope, &selection, html)?;
    transaction.changes().apply(&mut rope);
    let heads = transaction
        .selection()
        .expect("set")
        .iter()
        .map(|range| range.head)
        .collect();
    Some((rope.to_string(), heads))
}

#[test]
fn the_tag_typed_is_closed_after_the_caret() {
    assert_eq!(
        close("<p>", &[3], true),
        Some(("<p></p>".to_owned(), vec![3]))
    );
    // Two carets, each in its own tag: the second moves with the first tag inserted.
    assert_eq!(
        close("<a>\n<b>", &[3, 7], false),
        Some(("<a></a>\n<b></b>".to_owned(), vec![3, 11]))
    );
}

#[test]
fn nothing_to_close() {
    assert_eq!(close("<br>", &[4], true), None, "a void element");
    assert_eq!(close("</p>", &[4], true), None);
    assert_eq!(close("a > b", &[3], true), None);
    assert_eq!(close("", &[0], true), None);
    let rope = Rope::from_str("<p>");
    let selected = Selection::new(vec![Span::new(0, 3)], 0);
    assert!(close_tags(&rope, &selected, true).is_none(), "a selection");
}

/// How far back from the caret an opening tag is looked for.
const TAG_REACH: usize = 4096;

#[test]
fn a_tag_beyond_reach_is_left_alone() {
    let long = format!("<p title=\"{}\">", "x".repeat(TAG_REACH));
    assert_eq!(close(&long, &[long.len()], true), None);
    let just = format!("<p{}>", " ".repeat(TAG_REACH - 3));
    assert!(close(&just, &[just.len()], true).is_some());
}
