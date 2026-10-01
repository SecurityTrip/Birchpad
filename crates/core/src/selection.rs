//! Selections: one or more ranges, each with an anchor (where selecting started) and a head
//! (where the caret is). A single caret is a selection of one empty range.

use smallvec::SmallVec;

use crate::change::{Assoc, ChangeSet};

/// A selected range. `anchor == head` is a plain caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Range {
    pub anchor: usize,
    pub head: usize,
}

impl Range {
    pub const fn new(anchor: usize, head: usize) -> Self {
        Self { anchor, head }
    }

    pub const fn point(pos: usize) -> Self {
        Self {
            anchor: pos,
            head: pos,
        }
    }

    pub fn from(&self) -> usize {
        self.anchor.min(self.head)
    }

    pub fn to(&self) -> usize {
        self.anchor.max(self.head)
    }

    pub fn len(&self) -> usize {
        self.to() - self.from()
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// True if the range was selected right to left (the caret is at its start).
    pub fn is_backward(&self) -> bool {
        self.head < self.anchor
    }

    /// Maps the range through `changes`. Like Scintilla, text inserted exactly at a range edge
    /// does not push that edge.
    pub fn map(self, changes: &ChangeSet) -> Self {
        Self {
            anchor: changes.map_pos(self.anchor, Assoc::Before),
            head: changes.map_pos(self.head, Assoc::Before),
        }
    }

    /// Ranges starting at the same position or overlapping are merged; ranges that only touch
    /// are kept apart so that each caret still edits on its own.
    fn should_merge_with_next(&self, next: &Range) -> bool {
        next.from() < self.to() || next.from() == self.from()
    }

    fn merge(self, other: Range) -> Range {
        let from = self.from().min(other.from());
        let to = self.to().max(other.to());
        if self.is_backward() {
            Range::new(to, from)
        } else {
            Range::new(from, to)
        }
    }
}

/// One or more ranges, sorted by position and never overlapping, with one of them primary.
///
/// The primary range is the one the view scrolls to and the status bar reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    ranges: SmallVec<[Range; 1]>,
    primary: usize,
}

impl Selection {
    pub fn point(pos: usize) -> Self {
        Self::single(Range::point(pos))
    }

    pub fn single(range: Range) -> Self {
        Self {
            ranges: smallvec::smallvec![range],
            primary: 0,
        }
    }

    /// Builds a selection from ranges in any order. Overlapping ranges are merged.
    ///
    /// # Panics
    ///
    /// Panics if `ranges` is empty or `primary` is not an index into it.
    pub fn new(ranges: impl IntoIterator<Item = Range>, primary: usize) -> Self {
        let ranges: SmallVec<[Range; 1]> = ranges.into_iter().collect();
        assert!(!ranges.is_empty(), "a selection needs at least one range");
        assert!(primary < ranges.len(), "primary range index out of bounds");
        let mut selection = Self { ranges, primary };
        selection.normalize();
        selection
    }

    pub fn ranges(&self) -> &[Range] {
        &self.ranges
    }

    pub fn primary(&self) -> Range {
        self.ranges[self.primary]
    }

    pub fn primary_index(&self) -> usize {
        self.primary
    }

    pub fn iter(&self) -> impl Iterator<Item = &Range> {
        self.ranges.iter()
    }

    /// Adds a range and makes it primary, as Ctrl+click does.
    pub fn push(mut self, range: Range) -> Self {
        self.ranges.push(range);
        self.primary = self.ranges.len() - 1;
        self.normalize();
        self
    }

    /// Applies `f` to every range, keeping the primary one.
    pub fn transform(&self, f: impl FnMut(Range) -> Range) -> Self {
        Self::new(self.ranges.iter().copied().map(f), self.primary)
    }

    /// Maps every range through `changes`.
    pub fn map(&self, changes: &ChangeSet) -> Self {
        self.transform(|range| range.map(changes))
    }

    fn normalize(&mut self) {
        if self.ranges.len() == 1 {
            return;
        }
        let mut tagged: SmallVec<[(Range, bool); 4]> = self
            .ranges
            .iter()
            .enumerate()
            .map(|(i, &range)| (range, i == self.primary))
            .collect();
        tagged.sort_by_key(|(range, _)| (range.from(), range.to()));

        self.ranges.clear();
        self.primary = 0;
        for (range, is_primary) in tagged {
            match self.ranges.last_mut() {
                Some(last) if last.should_merge_with_next(&range) => {
                    *last = if is_primary {
                        range.merge(*last)
                    } else {
                        last.merge(range)
                    };
                }
                _ => self.ranges.push(range),
            }
            if is_primary {
                self.primary = self.ranges.len() - 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change::Edit;
    use ropey::Rope;

    #[test]
    fn sorts_and_merges_overlapping_ranges() {
        let selection = Selection::new(
            [
                Range::new(10, 12),
                Range::point(3),
                Range::new(1, 5),
                Range::point(12),
            ],
            1,
        );
        assert_eq!(
            selection.ranges(),
            [Range::new(1, 5), Range::new(10, 12), Range::point(12)]
        );
        assert_eq!(
            selection.primary(),
            Range::new(1, 5),
            "primary follows the merge"
        );
    }

    #[test]
    fn merged_range_keeps_direction_of_primary() {
        let selection = Selection::new([Range::new(0, 4), Range::new(6, 2)], 1);
        assert_eq!(selection.ranges(), [Range::new(6, 0)]);
    }

    #[test]
    fn duplicate_carets_collapse() {
        let selection = Selection::point(5).push(Range::point(5));
        assert_eq!(selection.ranges(), [Range::point(5)]);
    }

    #[test]
    fn push_makes_range_primary() {
        let selection = Selection::point(10).push(Range::point(2));
        assert_eq!(selection.ranges(), [Range::point(2), Range::point(10)]);
        assert_eq!(selection.primary(), Range::point(2));
    }

    #[test]
    fn maps_through_changes() {
        let rope = Rope::from_str("hello world");
        let changes = ChangeSet::from_edits(&rope, [Edit::insert(0, ">> ")]).unwrap();
        let selection = Selection::new([Range::point(0), Range::new(6, 11)], 1);
        assert_eq!(
            selection.map(&changes).ranges(),
            [Range::point(0), Range::new(9, 14)]
        );
    }
}
