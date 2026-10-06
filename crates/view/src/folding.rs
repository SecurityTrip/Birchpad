//! Folds of a view in lines: where they are, how deep, and which lines collapsed ones hide.

use std::ops::Range;

use birchpad_core::motion::{line_count, line_of};
use birchpad_core::{ChangeSet, Rope};

use crate::display_map::changed_range;

/// A fold point: when collapsed, lines `header + 1 ..= last` are hidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fold {
    pub header: usize,
    pub last: usize,
    /// Nesting depth, 1 for a fold not inside another, as in Notepad++'s "level".
    pub level: usize,
}

/// Fold points from byte ranges sorted by start (from the syntax tree or indentation). A range
/// that ends on its header line folds nothing and is skipped; of two folds on one header line
/// the longer one is kept.
pub fn folds_from_ranges(text: &Rope, ranges: &[Range<usize>]) -> Vec<Fold> {
    let mut folds: Vec<Fold> = Vec::with_capacity(ranges.len());
    for range in ranges {
        let end = range.end.min(text.len());
        if range.start >= end {
            continue;
        }
        let header = line_of(text, range.start);
        let last = line_of(text, text.floor_char_boundary(end - 1));
        if last <= header {
            continue;
        }
        match folds.last_mut() {
            Some(previous) if previous.header == header => previous.last = previous.last.max(last),
            _ => folds.push(Fold {
                header,
                last,
                level: 0,
            }),
        }
    }
    folds.sort_by_key(|fold| fold.header);
    // Levels: how many folds still open when each one starts.
    let mut open: Vec<usize> = Vec::new();
    for fold in &mut folds {
        while open.last().is_some_and(|&last| last < fold.header) {
            open.pop();
        }
        fold.level = open.len() + 1;
        open.push(fold.last);
    }
    folds
}

/// Follows an edit that produced `text` from a text of `old_lines` lines, without recomputing
/// every fold: folds before the edited lines stay, folds after them shift, folds around them
/// grow or shrink. Returns false if a fold starts or ends on an edited line; the folds must
/// be recomputed then.
pub fn shift_folds(folds: &mut [Fold], text: &Rope, changes: &ChangeSet, old_lines: usize) -> bool {
    let Some(changed) = changed_range(changes) else {
        return true;
    };
    let delta = line_count(text) as isize - old_lines as isize;
    let first = line_of(text, changed.start);
    // The last edited line, in old line numbers.
    let last_old = line_of(text, changed.end) as isize - delta;
    if last_old < first as isize {
        return false;
    }
    let last_old = last_old as usize;
    let shift = |line: usize| (line as isize + delta) as usize;
    for fold in folds.iter() {
        let touches = |line: usize| (first..=last_old).contains(&line);
        if touches(fold.header) || touches(fold.last) {
            return false;
        }
    }
    for fold in folds.iter_mut() {
        if fold.header > last_old {
            fold.header = shift(fold.header);
            fold.last = shift(fold.last);
        } else if fold.last > last_old {
            fold.last = shift(fold.last);
        }
    }
    true
}

/// The fold with header `line`, if `line` is a fold point.
pub fn fold_at(folds: &[Fold], line: usize) -> Option<&Fold> {
    folds
        .binary_search_by_key(&line, |fold| fold.header)
        .ok()
        .map(|index| &folds[index])
}

/// The innermost fold whose lines (header included) contain `line`.
pub fn innermost_containing(folds: &[Fold], line: usize) -> Option<&Fold> {
    let candidates = folds.partition_point(|fold| fold.header <= line);
    folds[..candidates]
        .iter()
        .rev()
        .find(|fold| fold.last >= line)
}

/// Lines hidden by the folds whose headers are in `collapsed`: sorted, disjoint line ranges.
/// Headers that are not (or no longer) fold points are ignored.
pub fn hidden_lines(folds: &[Fold], collapsed: &[usize]) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = collapsed
        .iter()
        .filter_map(|&line| fold_at(folds, line))
        .map(|fold| fold.header + 1..fold.last + 1)
        .collect();
    ranges.sort_by_key(|range| range.start);
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match merged.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => merged.push(range),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::single_range_in_vec_init,
        reason = "lists of fold ranges, some with one range"
    )]

    use super::*;

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
        let start =
            |text: &Rope, line: usize| text.line_to_byte_idx(line, birchpad_core::LINE_TYPE);
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
}
