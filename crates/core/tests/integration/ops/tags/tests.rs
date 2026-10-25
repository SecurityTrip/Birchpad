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

fn names(text: &str, caret: usize, in_comments: bool) -> Option<(String, String)> {
    let rope = ropey::Rope::from_str(text);
    let pair = matching_tag(&rope, caret, in_comments)?;
    Some((
        text[pair.open.range.clone()].to_owned(),
        text[pair.close.range.clone()].to_owned(),
    ))
}

#[test]
fn a_tag_finds_its_partner_across_nested_ones() {
    let text = "<div><div>in</div><p>x</p></div>";
    let pair = |caret| names(text, caret, false);
    // In the outer opening tag, and in its closing tag.
    assert_eq!(pair(2), Some(("<div>".into(), "</div>".into())));
    let rope = ropey::Rope::from_str(text);
    let outer = matching_tag(&rope, 2, false).unwrap();
    assert_eq!(outer.close.range.end, text.len());
    assert_eq!(outer.open.name, 1..4);
    let inner = matching_tag(&rope, 7, false).unwrap();
    assert_eq!(inner.open.range, 5..10);
    assert_eq!(inner.close.range, 12..18);
    let from_close = matching_tag(&rope, text.len() - 2, false).unwrap();
    assert_eq!(from_close.open.range, 0..5);
}

#[test]
fn attributes_and_self_closed_tags() {
    let text = "<a href=\"x\"><br/></a>";
    let rope = ropey::Rope::from_str(text);
    let pair = matching_tag(&rope, 3, false).unwrap();
    assert_eq!(&text[pair.open.range], "<a href=\"x\">");
    assert_eq!(&text[pair.open.name], "a");
    // A self-closed tag has no partner; a caret outside tags none either.
    assert!(matching_tag(&rope, 14, false).is_none());
    assert!(names("text <b>x</b>", 2, false).is_none());
    assert!(names("<b>never closed", 1, false).is_none());
    assert!(names("", 0, false).is_none());
}

#[test]
fn comments_count_only_when_asked() {
    let text = "<b><!-- <b> --></b>";
    // The tag in the comment is not counted: the outer <b> closes at the end.
    let rope = ropey::Rope::from_str(text);
    let pair = matching_tag(&rope, 1, false).unwrap();
    assert_eq!(pair.close.range.end, text.len());
    // With comments, the inner one is open and takes the closing tag.
    assert!(matching_tag(&rope, 1, true).is_none());
    assert!(matching_tag(&rope, 9, true).is_some());
}
