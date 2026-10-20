#![allow(
    clippy::single_range_in_vec_init,
    reason = "lists of fold ranges, some with one range"
)]

use birchpad_core::ChangeSet;
use birchpad_core::Rope;
use birchpad_core::motion::line_count;
use birchpad_view::*;

fn folds() -> Vec<Fold> {
    // 0 a {        1 b {   2 c   3 }   4 d {   5 e   6 }   7 }   8 f {   9 }
    let text = Rope::from_str("a {\nb {\nc\n}\nd {\ne\n}\n}\nf {\n}\n");
    let start = |line: usize| text.line_to_byte_idx(line, birchpad_core::LINE_TYPE);
    let end = |line: usize| birchpad_core::motion::line_range(&text, line).end;
    folds_from_ranges(
        &text,
        &[
            start(0)..end(7),
            start(1)..end(3),
            start(4)..end(6),
            start(8)..end(9),
            // A one-line range folds nothing.
            start(9)..end(9),
        ],
    )
}

#[test]
fn ranges_that_fold_nothing_are_dropped() {
    let text = Rope::from_str("a {\nb\n}");
    // Empty, reversed, past the end of the text, on one line.
    #[expect(clippy::reversed_empty_ranges, reason = "a reversed range is the case")]
    let reversed = 6..2;
    let ranges = [4..4, reversed, 99..120, 0..3];
    assert!(folds_from_ranges(&text, &ranges).is_empty());
    // One that runs past the end is cut there.
    let folds = folds_from_ranges(&text, &[2..999]);
    assert_eq!(
        folds.iter().map(|f| (f.header, f.last)).collect::<Vec<_>>(),
        [(0, 2)]
    );
    assert!(folds_from_ranges(&Rope::new(), &[0..5]).is_empty());
}

#[test]
fn an_edit_that_changes_nothing_keeps_the_folds() {
    let text = Rope::from_str("a {\nb\n}");
    let mut folds = folds_from_ranges(&text, &[2..7]);
    let before = folds.clone();
    let identity = ChangeSet::identity(text.len());
    assert!(shift_folds(&mut folds, &text, &identity, line_count(&text)));
    assert_eq!(folds, before);
}

#[test]
fn levels_count_nesting() {
    let levels: Vec<(usize, usize, usize)> = folds()
        .iter()
        .map(|fold| (fold.header, fold.last, fold.level))
        .collect();
    assert_eq!(levels, [(0, 7, 1), (1, 3, 2), (4, 6, 2), (8, 9, 1)]);
}

#[test]
fn lookups() {
    let folds = folds();
    assert_eq!(fold_at(&folds, 4).map(|fold| fold.last), Some(6));
    assert_eq!(fold_at(&folds, 5), None);
    assert_eq!(innermost_containing(&folds, 5).map(|f| f.header), Some(4));
    assert_eq!(innermost_containing(&folds, 7).map(|f| f.header), Some(0));
    assert_eq!(innermost_containing(&folds, 10), None);
}

#[test]
fn edits_shift_folds_without_recomputing() {
    let mut text = Rope::from_str(
        "a {
b {
c
}
d {
e
}
}
f {
}
",
    );
    let start = |text: &Rope, line: usize| text.line_to_byte_idx(line, birchpad_core::LINE_TYPE);
    let mut folds = folds();
    let lines = line_count(&text);
    // Two lines typed inside `d { e }` (line 5).
    let at = start(&text, 5);
    let changes = ChangeSet::from_edits(
        &text,
        [birchpad_core::Edit::insert(
            at, "x
y
",
        )],
    )
    .unwrap();
    changes.apply(&mut text);
    assert!(shift_folds(&mut folds, &text, &changes, lines));
    let shifted: Vec<(usize, usize)> = folds.iter().map(|f| (f.header, f.last)).collect();
    assert_eq!(shifted, [(0, 9), (1, 3), (4, 8), (10, 11)]);

    // Editing a header line needs a recompute.
    let lines = line_count(&text);
    let at = start(&text, 4);
    let changes = ChangeSet::from_edits(&text, [birchpad_core::Edit::insert(at, "z")]).unwrap();
    changes.apply(&mut text);
    assert!(!shift_folds(&mut folds, &text, &changes, lines));
}

#[test]
fn hidden_lines_merge_nested_folds() {
    let folds = folds();
    assert_eq!(hidden_lines(&folds, &[1, 4]), [2..4, 5..7]);
    // A collapsed fold inside a collapsed one changes nothing more.
    assert_eq!(hidden_lines(&folds, &[1, 0, 8]), [1..8, 9..10]);
    assert!(hidden_lines(&folds, &[2]).is_empty(), "not a fold point");
}
