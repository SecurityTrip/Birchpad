//! Decorations that follow the text as it is edited: styled ranges (like Scintilla's
//! indicators: Mark styles, search matches) and line markers (like Scintilla's markers:
//! bookmarks).
//!
//! Both are plain data mapped through every [`ChangeSet`]; how they look is up to the UI.

use std::ops::Range;

use ropey::Rope;

use crate::change::{Assoc, ChangeSet};
use crate::motion::{line_count, line_of};

/// Non-overlapping, non-empty ranges sorted by position, each with a value.
///
/// Setting a value over a range replaces whatever was there, splitting ranges it partly
/// covers, the way Scintilla fills an indicator. Ranges of equal value that touch are kept
/// apart (two adjacent matches stay two matches).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeSet<T> {
    ranges: Vec<(Range<usize>, T)>,
}

impl<T> Default for RangeSet<T> {
    fn default() -> Self {
        Self { ranges: Vec::new() }
    }
}

impl<T: Clone> RangeSet<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a set from ranges sorted by start that do not overlap (e.g. search matches).
    /// Empty ranges are dropped.
    ///
    /// # Panics
    ///
    /// If the ranges are unsorted or overlap.
    pub fn from_sorted(ranges: impl IntoIterator<Item = (Range<usize>, T)>) -> Self {
        let ranges: Vec<_> = ranges.into_iter().filter(|(r, _)| !r.is_empty()).collect();
        assert!(
            ranges.windows(2).all(|w| w[0].0.end <= w[1].0.start),
            "ranges must be sorted and disjoint"
        );
        Self { ranges }
    }

    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    pub fn clear(&mut self) {
        self.ranges.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = (Range<usize>, &T)> {
        self.ranges
            .iter()
            .map(|(range, value)| (range.clone(), value))
    }

    /// Ranges that intersect `range` (or touch it, for an empty `range`), in order.
    pub fn overlapping(&self, range: Range<usize>) -> impl Iterator<Item = (Range<usize>, &T)> {
        let first = self.ranges.partition_point(|(r, _)| r.end <= range.start);
        self.ranges[first..]
            .iter()
            .take_while(move |(r, _)| {
                r.start < range.end || (range.is_empty() && r.start == range.start)
            })
            .map(|(r, value)| (r.clone(), value))
    }

    /// The value at `pos`, if a range covers it.
    pub fn at(&self, pos: usize) -> Option<&T> {
        let index = self.ranges.partition_point(|(r, _)| r.end <= pos);
        self.ranges
            .get(index)
            .filter(|(r, _)| r.start <= pos)
            .map(|(_, value)| value)
    }

    /// Sets `value` over `range`, replacing what was there.
    pub fn insert(&mut self, range: Range<usize>, value: T) {
        if range.is_empty() {
            return;
        }
        let index = self.remove_inner(range.clone());
        self.ranges.insert(index, (range, value));
    }

    /// Clears `range`, trimming or splitting the ranges it covers.
    pub fn remove(&mut self, range: Range<usize>) {
        if !range.is_empty() {
            self.remove_inner(range);
        }
    }

    /// Removes everything inside `range` and returns where a range starting there would go.
    fn remove_inner(&mut self, range: Range<usize>) -> usize {
        let first = self.ranges.partition_point(|(r, _)| r.end <= range.start);
        let last = self.ranges.partition_point(|(r, _)| r.start < range.end);
        if first >= last {
            return first;
        }
        let mut keep = Vec::with_capacity(2);
        let (head, head_value) = self.ranges[first].clone();
        if head.start < range.start {
            keep.push((head.start..range.start, head_value));
        }
        let (tail, tail_value) = self.ranges[last - 1].clone();
        if tail.end > range.end {
            keep.push((range.end..tail.end, tail_value));
        }
        let insert_at = first + usize::from(head.start < range.start);
        self.ranges.splice(first..last, keep);
        insert_at
    }

    /// The first range starting at or after `pos`.
    pub fn next_from(&self, pos: usize) -> Option<(Range<usize>, &T)> {
        let index = self.ranges.partition_point(|(r, _)| r.start < pos);
        self.ranges.get(index).map(|(r, value)| (r.clone(), value))
    }

    /// The last range ending at or before `pos`.
    pub fn previous_to(&self, pos: usize) -> Option<(Range<usize>, &T)> {
        let index = self.ranges.partition_point(|(r, _)| r.end <= pos);
        index
            .checked_sub(1)
            .map(|i| (self.ranges[i].0.clone(), &self.ranges[i].1))
    }

    /// Follows an edit. Text inserted inside a range extends it; text inserted at either edge
    /// does not, as with Scintilla's indicators. Ranges whose text was deleted disappear.
    pub fn map(&mut self, changes: &ChangeSet) {
        if changes.is_identity() || self.ranges.is_empty() {
            return;
        }
        // Starts and ends are both non-decreasing in a sorted disjoint set, so each takes one
        // pass of a mapper.
        let mut starts = changes.mapper(Assoc::After);
        let mut ends = changes.mapper(Assoc::Before);
        for (range, _) in &mut self.ranges {
            let start = starts.map(range.start);
            let end = ends.map(range.end).max(start);
            *range = start..end;
        }
        self.ranges.retain(|(range, _)| !range.is_empty());
    }
}

/// Maps ranges that may nest (fold points), sorted by start, through `changes` with the
/// semantics of [`RangeSet::map`]: text inserted inside a range extends it, text inserted at an
/// edge does not, and ranges whose text was deleted disappear. Stays sorted by start.
pub fn map_ranges(ranges: &mut Vec<Range<usize>>, changes: &ChangeSet) {
    if changes.is_identity() || ranges.is_empty() {
        return;
    }
    // Ends are not sorted when ranges nest: map them in sorted order through one mapper.
    let mut by_end: Vec<usize> = (0..ranges.len()).collect();
    by_end.sort_by_key(|&index| ranges[index].end);
    let mut ends = changes.mapper(Assoc::Before);
    let mapped_ends: Vec<(usize, usize)> = by_end
        .into_iter()
        .map(|index| (index, ends.map(ranges[index].end)))
        .collect();
    let mut starts = changes.mapper(Assoc::After);
    for range in ranges.iter_mut() {
        range.start = starts.map(range.start);
    }
    for (index, end) in mapped_ends {
        ranges[index].end = end.max(ranges[index].start);
    }
    ranges.retain(|range| !range.is_empty());
}

/// Lines with a marker (bookmarks), tracked through edits.
///
/// Each marker is a position on its line, so it moves with the line's text: typing at the
/// start of the line or pressing Enter there takes the marker along with the text. When a
/// marked line is deleted, the marker lands on the line that takes its place, as Scintilla
/// merges the markers of deleted lines.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineMarkers {
    /// Sorted positions, at most one per line.
    positions: Vec<usize>,
}

impl LineMarkers {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    pub fn len(&self) -> usize {
        self.positions.len()
    }

    pub fn clear(&mut self) {
        self.positions.clear();
    }

    /// The marked lines, in order.
    pub fn lines(&self, text: &Rope) -> Vec<usize> {
        self.positions
            .iter()
            .map(|&pos| line_of(text, pos))
            .collect()
    }

    /// Marked lines among `lines` (a range of line numbers), in order.
    pub fn lines_in(&self, text: &Rope, lines: Range<usize>) -> Vec<usize> {
        if lines.is_empty() {
            return Vec::new();
        }
        let start = line_start(text, lines.start);
        let end = if lines.end >= line_count(text) {
            usize::MAX
        } else {
            line_start(text, lines.end)
        };
        let first = self.positions.partition_point(|&pos| pos < start);
        self.positions[first..]
            .iter()
            .take_while(|&&pos| pos < end)
            .map(|&pos| line_of(text, pos))
            .collect()
    }

    pub fn contains(&self, text: &Rope, line: usize) -> bool {
        self.index_of(text, line).is_ok()
    }

    /// Adds a marker to `line`; returns false if it already had one.
    pub fn add(&mut self, text: &Rope, line: usize) -> bool {
        match self.index_of(text, line) {
            Ok(_) => false,
            Err(index) => {
                self.positions.insert(index, line_start(text, line));
                true
            }
        }
    }

    /// Removes the marker of `line`; returns false if it had none.
    pub fn remove(&mut self, text: &Rope, line: usize) -> bool {
        match self.index_of(text, line) {
            Ok(index) => {
                self.positions.remove(index);
                true
            }
            Err(_) => false,
        }
    }

    /// Adds or removes the marker of `line`; returns whether it is now marked.
    pub fn toggle(&mut self, text: &Rope, line: usize) -> bool {
        if self.remove(text, line) {
            false
        } else {
            self.add(text, line)
        }
    }

    /// The first marked line after `line`, wrapping around to the start.
    pub fn next_after(&self, text: &Rope, line: usize) -> Option<usize> {
        let lines = self.lines(text);
        lines
            .iter()
            .copied()
            .find(|&marked| marked > line)
            .or_else(|| lines.first().copied())
    }

    /// The last marked line before `line`, wrapping around to the end.
    pub fn previous_before(&self, text: &Rope, line: usize) -> Option<usize> {
        let lines = self.lines(text);
        lines
            .iter()
            .rev()
            .copied()
            .find(|&marked| marked < line)
            .or_else(|| lines.last().copied())
    }

    /// Follows an edit that produced `text`. Markers that end up on the same line merge.
    pub fn map(&mut self, changes: &ChangeSet, text: &Rope) {
        if changes.is_identity() || self.positions.is_empty() {
            return;
        }
        let mut mapper = changes.mapper(Assoc::After);
        for pos in &mut self.positions {
            *pos = mapper.map(*pos);
        }
        let mut last_line = None;
        self.positions.retain(|&pos| {
            let line = line_of(text, pos);
            let keep = last_line != Some(line);
            last_line = Some(line);
            keep
        });
    }

    /// Where the marker of `line` is (`Ok`) or would go (`Err`) in `positions`.
    fn index_of(&self, text: &Rope, line: usize) -> Result<usize, usize> {
        let start = line_start(text, line);
        let index = self.positions.partition_point(|&pos| pos < start);
        match self.positions.get(index) {
            Some(&pos) if line_of(text, pos) == line => Ok(index),
            _ => Err(index),
        }
    }
}

fn line_start(text: &Rope, line: usize) -> usize {
    text.line_to_byte_idx(line.min(line_count(text) - 1), crate::motion::LINE_TYPE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change::Edit;

    fn edit(text: &mut Rope, edits: Vec<Edit>) -> ChangeSet {
        let changes = ChangeSet::from_edits(text, edits).unwrap();
        changes.apply(text);
        changes
    }

    fn ranges(set: &RangeSet<u8>) -> Vec<(Range<usize>, u8)> {
        set.iter().map(|(r, v)| (r, *v)).collect()
    }

    #[test]
    fn insert_replaces_and_splits() {
        let mut set = RangeSet::new();
        set.insert(0..10, 1);
        set.insert(3..5, 2);
        assert_eq!(ranges(&set), [(0..3, 1), (3..5, 2), (5..10, 1)]);
        set.insert(4..8, 3);
        assert_eq!(ranges(&set), [(0..3, 1), (3..4, 2), (4..8, 3), (8..10, 1)]);
        set.remove(2..9);
        assert_eq!(ranges(&set), [(0..2, 1), (9..10, 1)]);
        set.insert(2..9, 4);
        assert_eq!(ranges(&set), [(0..2, 1), (2..9, 4), (9..10, 1)]);
    }

    #[test]
    fn queries() {
        let set = RangeSet::from_sorted([(2..4, 1), (6..9, 2), (9..10, 3)]);
        assert_eq!(set.at(3), Some(&1));
        assert_eq!(set.at(4), None);
        assert_eq!(set.at(9), Some(&3));
        let found: Vec<_> = set.overlapping(3..7).map(|(r, _)| r).collect();
        assert_eq!(found, [2..4, 6..9]);
        assert_eq!(set.next_from(5).map(|(r, _)| r), Some(6..9));
        assert_eq!(set.next_from(10), None);
        assert_eq!(set.previous_to(6).map(|(r, _)| r), Some(2..4));
        assert_eq!(set.previous_to(2), None);
    }

    #[test]
    fn ranges_follow_edits_like_indicators() {
        let mut text = Rope::from_str("one two three");
        let mut set = RangeSet::from_sorted([(4..7, 1), (8..13, 2)]);
        // Inserting at the edges does not extend a range; inside it does.
        let changes = edit(&mut text, vec![Edit::insert(4, "<"), Edit::insert(7, ">")]);
        set.map(&changes);
        assert_eq!(ranges(&set), [(5..8, 1), (10..15, 2)]);
        let changes = edit(&mut text, vec![Edit::insert(6, "w")]);
        set.map(&changes);
        assert_eq!(ranges(&set), [(5..9, 1), (11..16, 2)]);
        // Deleting a range's text removes it.
        let changes = edit(&mut text, vec![Edit::delete(4..11)]);
        set.map(&changes);
        assert_eq!(ranges(&set), [(4..9, 2)]);
        assert_eq!(text, "one three");
    }

    #[test]
    fn markers_follow_their_lines() {
        let mut text = Rope::from_str("a\nb\nc\nd");
        let mut markers = LineMarkers::new();
        assert!(markers.add(&text, 1));
        assert!(markers.add(&text, 3));
        assert!(!markers.add(&text, 3));
        assert_eq!(markers.lines(&text), [1, 3]);

        // Enter at the start of a marked line moves the marker down with the text.
        let changes = edit(&mut text, vec![Edit::insert(2, "\n")]);
        markers.map(&changes, &text);
        assert_eq!(markers.lines(&text), [2, 4]);

        // Deleting a marked line leaves the marker on the line that takes its place, and
        // markers landing on one line merge.
        let changes = edit(&mut text, vec![Edit::delete(3..7)]);
        markers.map(&changes, &text);
        assert_eq!(text, "a\n\nd");
        assert_eq!(markers.lines(&text), [2]);

        assert!(!markers.toggle(&text, 2));
        assert!(markers.is_empty());
    }

    #[test]
    fn marker_navigation_wraps() {
        let text = Rope::from_str("0\n1\n2\n3\n4");
        let mut markers = LineMarkers::new();
        markers.add(&text, 1);
        markers.add(&text, 3);
        assert_eq!(markers.next_after(&text, 1), Some(3));
        assert_eq!(markers.next_after(&text, 3), Some(1));
        assert_eq!(markers.previous_before(&text, 1), Some(3));
        assert_eq!(markers.previous_before(&text, 2), Some(1));
        assert_eq!(markers.lines_in(&text, 2..5), [3]);
        assert_eq!(markers.lines_in(&text, 0..2), [1]);
    }
}
