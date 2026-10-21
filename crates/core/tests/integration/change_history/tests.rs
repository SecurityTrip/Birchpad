#![allow(
    clippy::single_range_in_vec_init,
    reason = "one range of lines is the expected result"
)]

use proptest::prelude::*;

use std::ops::Range;

use birchpad_core::*;

/// `ranges` sorted, those that overlap or touch joined.
fn merged(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.sort_unstable_by_key(|range| range.start);
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        match merged.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => merged.push(range),
        }
    }
    merged
}

fn rope(text: &str) -> Rope {
    Rope::from_str(text)
}

fn changed(old: &str, new: &str) -> Vec<Range<usize>> {
    diff_lines(&rope(old), &rope(new)).changed
}

fn history(original: &str) -> ChangeHistory {
    ChangeHistory::new(rope(original))
}

use LineChange::{Modified, Saved};
use birchpad_core::motion::line_count;
use std::time::Instant;

const NO_LINES: [Range<usize>; 0] = [];
const NO_CHANGES: [(Range<usize>, LineChange); 0] = [];

#[test]
fn equal_texts_have_no_changes() {
    assert_eq!(changed("", ""), NO_LINES);
    assert_eq!(changed("a\nb\n", "a\nb\n"), NO_LINES);
    assert_eq!(diff_lines(&rope("a\nb"), &rope("a\nb")).equal, [(0, 0, 2)]);
}

#[test]
fn inserted_and_replaced_lines_change() {
    assert_eq!(changed("a\nc\n", "a\nb\nc\n"), [1..2]);
    assert_eq!(changed("a\nb\nc", "a\nB\nc"), [1..2]);
    assert_eq!(changed("", "x"), [0..1]);
    // Typing at the end of the last line, and Enter there.
    assert_eq!(changed("a\nb", "a\nbc"), [1..2]);
    assert_eq!(changed("a\nb", "a\nb\n"), [1..3]);
}

#[test]
fn deleted_lines_mark_the_line_in_their_place() {
    assert_eq!(changed("a\nb\nc\n", "a\nc\n"), [1..2]);
    // At the end: the last line.
    assert_eq!(changed("a\nb\n", "a\n"), [1..2]);
    assert_eq!(changed("a\nb", "a"), [0..1]);
}

#[test]
fn line_breaks_are_part_of_the_line() {
    assert_eq!(changed("a\r\nb\r\n", "a\nb\n"), [0..2]);
}

#[test]
fn equal_runs_pair_the_lines() {
    let diff = diff_lines(&rope("a\nb\nc\nd"), &rope("a\nx\ny\nc\nd"));
    assert_eq!(diff.changed, [1..3]);
    assert_eq!(diff.equal, [(0, 0, 1), (2, 3, 2)]);
}

#[test]
fn edits_are_modified_until_saved_then_saved() {
    let mut history = history("one\ntwo\nthree\n");
    let edited = rope("one\n2\nthree\n");
    assert_eq!(history.lines(&edited), [(1..2, Modified)]);
    history.save(edited.clone());
    assert_eq!(history.lines(&edited), [(1..2, Saved)]);

    // A line added above moves the saved change down; editing it again makes it modified.
    let above = rope("zero\none\n2\nthree\n");
    assert_eq!(history.lines(&above), [(0..1, Modified), (2..3, Saved)]);
    let again = rope("one\ntwo again\nthree\n");
    assert_eq!(history.lines(&again), [(1..2, Modified)]);
}

#[test]
fn undoing_to_the_saved_text_clears_the_modified_lines() {
    let mut history = history("a\nb\n");
    assert_eq!(history.lines(&rope("a\nB\n")), [(1..2, Modified)]);
    assert_eq!(history.lines(&rope("a\nb\n")), NO_CHANGES);
}

#[test]
fn saving_the_original_text_again_clears_the_saved_changes() {
    let mut history = history("a\nb\n");
    history.save(rope("a\nB\n"));
    assert_eq!(history.lines(&rope("a\nB\n")), [(1..2, Saved)]);
    history.save(rope("a\nb\n"));
    assert_eq!(history.lines(&rope("a\nb\n")), NO_CHANGES);
}

#[test]
fn appended_text_is_not_a_change() {
    let mut history = history("log 1\n");
    history.append("log 2\n");
    assert_eq!(history.lines(&rope("log 1\nlog 2\n")), NO_CHANGES);
}

#[test]
fn lines_modified_again_cut_holes_in_the_saved_ones() {
    let mut history = history(
        "a
b
c
d
e
",
    );
    history.save(rope(
        "A
B
C
D
E
",
    ));
    assert_eq!(
        history.lines(&rope(
            "A
B
x
D
y
"
        )),
        [
            (0..2, Saved),
            (2..3, Modified),
            (3..4, Saved),
            (4..5, Modified)
        ]
    );
    // Every saved line modified again: none is left saved.
    assert_eq!(
        history.lines(&rope(
            "1
2
3
4
5
"
        )),
        [(0..5, Modified)]
    );
}

/// The lines of `new` the diff says are equal really are, and every other line is changed.
fn check(old: &str, new: &str) {
    let (old, new) = (rope(old), rope(new));
    let diff = diff_lines(&old, &new);
    let mut covered = vec![false; line_count(&new)];
    for &(o, n, len) in &diff.equal {
        for i in 0..len {
            assert_eq!(
                old.line(o + i, LINE_TYPE),
                new.line(n + i, LINE_TYPE),
                "equal run {o} {n} {len}"
            );
            assert!(!covered[n + i]);
            covered[n + i] = true;
        }
    }
    for range in &diff.changed {
        for line in range.clone() {
            covered[line] = true;
        }
    }
    assert!(covered.iter().all(|&c| c), "every line is equal or changed");
    // Runs are in order in both texts.
    for pair in diff.equal.windows(2) {
        assert!(pair[0].0 + pair[0].2 <= pair[1].0);
        assert!(pair[0].1 + pair[0].2 <= pair[1].1);
    }
}

#[test]
#[ignore = "a timing; run in release: cargo test --release -- --ignored --nocapture"]
fn change_history_of_a_large_file() {
    let line = "a line of some forty characters of text\n";
    let original = rope(&line.repeat(250_000));
    let mut history = ChangeHistory::new(original.clone());
    let mut edited = original.clone();
    edited.insert(original.len() / 2, "x");
    let started = Instant::now();
    let lines = history.lines(&edited);
    birchpad_core::timing::within(
        "change history, one edit in 10 MB",
        started.elapsed(),
        std::time::Duration::from_millis(500),
    );
    assert_eq!(lines, [(125_000..125_001, Modified)]);

    // Edits at both ends: the whole text is compared.
    edited.insert(0, "x");
    edited.insert(edited.len(), "x");
    let started = Instant::now();
    let lines = history.lines(&edited);
    birchpad_core::timing::within(
        "change history, edits at both ends of 10 MB",
        started.elapsed(),
        std::time::Duration::from_secs(5),
    );
    assert_eq!(lines.len(), 3);
}

proptest! {
    #[test]
    fn diffs_pair_equal_lines_and_cover_the_rest(
        old in "[ab\r\n]{0,40}",
        new in "[ab\r\n]{0,40}",
    ) {
        check(&old, &new);
    }

    #[test]
    fn history_lines_are_sorted_and_disjoint(
        original in "[ab\r\n]{0,30}",
        saved in "[ab\r\n]{0,30}",
        current in "[ab\r\n]{0,30}",
    ) {
        let mut history = history(&original);
        history.save(rope(&saved));
        let current = rope(&current);
        let lines = history.lines(&current);
        for pair in lines.windows(2) {
            prop_assert!(pair[0].0.end <= pair[1].0.start);
        }
        for (range, _) in &lines {
            prop_assert!(!range.is_empty() && range.end <= line_count(&current));
        }
        // Modified lines are exactly the ones a diff against the saved text finds.
        let modified: Vec<_> = lines
            .iter()
            .filter(|(_, change)| *change == Modified)
            .map(|(range, _)| range.clone())
            .collect();
        prop_assert_eq!(merged(modified), diff_lines(&rope(&saved), &current).changed);
    }
}
