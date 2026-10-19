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
#[path = "../tests/unit/folding/tests.rs"]
mod tests;
