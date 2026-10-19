#![allow(
    clippy::single_range_in_vec_init,
    clippy::type_complexity,
    reason = "lists of one expected match, a summary of each line"
)]

use super::*;

#[test]
fn lines_gather_their_matches() {
    let text = Rope::from_str("foo bar foo\r\nbaz\nfoo-\nfoo\nbar");
    let matches = [0..3, 8..11, 17..20, 22..29];
    let hits = line_hits(&text, &matches);
    let summary: Vec<(usize, &str, Vec<Range<usize>>, Range<usize>, usize)> = hits
        .iter()
        .map(|hit| {
            (
                hit.line,
                hit.text.as_str(),
                hit.highlights.clone(),
                hit.target.clone(),
                hit.matches,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (0, "foo bar foo", vec![0..3, 8..11], 0..3, 2),
            (2, "foo-", vec![0..3], 0..3, 1),
            // A match over a line break shows its first line's part.
            (3, "foo", vec![0..3], 0..7, 1),
        ]
    );
}

#[test]
fn long_lines_are_cut() {
    let long = format!("{}needle", "x".repeat(2000));
    let text = Rope::from_str(&long);
    let hits = line_hits(&text, &[2000..2006]);
    assert_eq!(hits[0].text.len(), MAX_LINE_SHOWN);
    assert!(hits[0].highlights.is_empty(), "the match is past the cut");
    assert_eq!(hits[0].target, 2000..2006, "but going there still works");
}

#[test]
fn no_matches_no_lines() {
    assert!(line_hits(&Rope::from_str("abc"), &[]).is_empty());
    let empty = Rope::new();
    let hits = line_hits(&empty, &[0..0]);
    assert_eq!(hits.len(), 1, "an empty match on an empty text");
    assert_eq!(
        (hits[0].line, hits[0].text.as_str(), hits[0].matches),
        (0, "", 1)
    );
    assert!(hits[0].highlights.is_empty(), "nothing to highlight");
}

#[test]
fn a_line_exactly_as_long_as_shown_is_whole() {
    let line = "y".repeat(MAX_LINE_SHOWN);
    let text = Rope::from_str(&line);
    let hits = line_hits(&text, &[MAX_LINE_SHOWN - 1..MAX_LINE_SHOWN]);
    assert_eq!(hits[0].text, line);
    assert_eq!(hits[0].highlights, [MAX_LINE_SHOWN - 1..MAX_LINE_SHOWN]);
    // A cut never splits a character.
    let wide = format!("{}ж", "x".repeat(MAX_LINE_SHOWN - 1));
    let hits = line_hits(&Rope::from_str(&wide), &[0..1]);
    assert_eq!(hits[0].text.len(), MAX_LINE_SHOWN - 1);
}
