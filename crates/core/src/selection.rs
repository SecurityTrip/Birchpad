//! Selections: one or more ranges, each with an anchor (where selecting started) and a head
//! (where the caret is). A single caret is a selection of one empty range.

use ropey::Rope;
use smallvec::SmallVec;

use crate::change::{Assoc, ChangeSet};
use crate::motion::{line_of, line_range};

/// A selected range. `anchor == head` is a plain caret.
///
/// In a rectangular selection a range can reach past the end of its line into *virtual space*,
/// as in Scintilla: `anchor_virtual` and `head_virtual` count the cells between the end of the
/// line and the anchor or the head. They are 0 everywhere else, and can only be non-zero for a
/// position at the end of a line. Typing there first fills the virtual space with spaces.
///
/// Byte positions (`from`, `to`) never include virtual space, so code that only cares about
/// the text can ignore it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Range {
    pub anchor: usize,
    pub head: usize,
    pub anchor_virtual: usize,
    pub head_virtual: usize,
}

impl Range {
    pub const fn new(anchor: usize, head: usize) -> Self {
        Self {
            anchor,
            head,
            anchor_virtual: 0,
            head_virtual: 0,
        }
    }

    pub const fn point(pos: usize) -> Self {
        Self::new(pos, pos)
    }

    /// A caret `virtual_cells` past position `pos` (the end of a line).
    pub const fn virtual_point(pos: usize, virtual_cells: usize) -> Self {
        Self::new(pos, pos).with_virtual(virtual_cells, virtual_cells)
    }

    /// The same range with the given virtual space at the anchor and the head.
    pub const fn with_virtual(mut self, anchor_virtual: usize, head_virtual: usize) -> Self {
        self.anchor_virtual = anchor_virtual;
        self.head_virtual = head_virtual;
        self
    }

    /// The same range without virtual space.
    pub const fn without_virtual(self) -> Self {
        self.with_virtual(0, 0)
    }

    pub fn has_virtual(&self) -> bool {
        self.anchor_virtual > 0 || self.head_virtual > 0
    }

    /// Anchor and head as (byte, virtual cells), which orders them along the text.
    fn anchor_point(&self) -> (usize, usize) {
        (self.anchor, self.anchor_virtual)
    }

    fn head_point(&self) -> (usize, usize) {
        (self.head, self.head_virtual)
    }

    pub fn from(&self) -> usize {
        self.anchor.min(self.head)
    }

    pub fn to(&self) -> usize {
        self.anchor.max(self.head)
    }

    /// Virtual space at the start of the range.
    pub fn from_virtual(&self) -> usize {
        self.anchor_point().min(self.head_point()).1
    }

    /// Virtual space at the end of the range.
    pub fn to_virtual(&self) -> usize {
        self.anchor_point().max(self.head_point()).1
    }

    pub fn len(&self) -> usize {
        self.to() - self.from()
    }

    /// True for a caret: nothing selected, not even virtual space.
    pub fn is_empty(&self) -> bool {
        self.anchor_point() == self.head_point()
    }

    /// True if the range was selected right to left (the caret is at its start).
    pub fn is_backward(&self) -> bool {
        self.head_point() < self.anchor_point()
    }

    /// Maps the range through `changes`. Like Scintilla, text inserted exactly at a range edge
    /// does not push that edge; it does use up that edge's virtual space.
    pub fn map(self, changes: &ChangeSet) -> Self {
        let map = |pos: usize, virtual_cells: usize| {
            let mapped = changes.map_pos(pos, Assoc::Before);
            let inserted_here = virtual_cells > 0 && changes.map_pos(pos, Assoc::After) != mapped;
            (mapped, if inserted_here { 0 } else { virtual_cells })
        };
        let (anchor, anchor_virtual) = map(self.anchor, self.anchor_virtual);
        let (head, head_virtual) = map(self.head, self.head_virtual);
        Self {
            anchor,
            head,
            anchor_virtual,
            head_virtual,
        }
    }

    /// Drops virtual space at positions that are not at the end of a line of `text`.
    pub fn clip_virtual(self, text: &Rope) -> Self {
        let at_line_end = |pos: usize| {
            pos <= text.len() && line_range(text, line_of(text, pos.min(text.len()))).end == pos
        };
        let mut range = self;
        if range.anchor_virtual > 0 && !at_line_end(range.anchor) {
            range.anchor_virtual = 0;
        }
        if range.head_virtual > 0 && !at_line_end(range.head) {
            range.head_virtual = 0;
        }
        range
    }

    /// Ranges starting at the same position or overlapping are merged; ranges that only touch
    /// are kept apart so that each caret still edits on its own. Virtual space counts for
    /// overlapping; two ranges never start at the same byte.
    fn should_merge_with_next(&self, next: &Range) -> bool {
        (next.from(), next.from_virtual()) < (self.to(), self.to_virtual())
            || next.from() == self.from()
    }

    fn merge(self, other: Range) -> Range {
        let start = self
            .anchor_point()
            .min(self.head_point())
            .min(other.anchor_point().min(other.head_point()));
        let end = self
            .anchor_point()
            .max(self.head_point())
            .max(other.anchor_point().max(other.head_point()));
        let (anchor, head) = if self.is_backward() {
            (end, start)
        } else {
            (start, end)
        };
        Range::new(anchor.0, head.0).with_virtual(anchor.1, head.1)
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

    /// Drops virtual space that is not at the end of a line of `text` (after an edit).
    pub fn clip_virtual(&self, text: &Rope) -> Self {
        if !self.has_virtual() {
            return self.clone();
        }
        self.transform(|range| range.clip_virtual(text))
    }

    pub fn has_virtual(&self) -> bool {
        self.ranges.iter().any(Range::has_virtual)
    }

    /// The same ranges with another one primary.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of bounds.
    pub fn with_primary_index(mut self, index: usize) -> Self {
        assert!(
            index < self.ranges.len(),
            "primary range index out of bounds"
        );
        self.primary = index;
        self
    }

    /// The selection without range `index`, or `None` if it was the only one. If it was the
    /// primary range, the range before it becomes primary.
    pub fn remove(&self, index: usize) -> Option<Self> {
        if self.ranges.len() <= 1 || index >= self.ranges.len() {
            return None;
        }
        let mut ranges = self.ranges.clone();
        ranges.remove(index);
        let primary = if self.primary > index || (self.primary == index && index > 0) {
            self.primary - 1
        } else {
            self.primary
        };
        Some(Self {
            ranges,
            primary: primary.min(self.ranges.len() - 2),
        })
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
        tagged.sort_by_key(|(range, _)| {
            (
                range.from(),
                range.from_virtual(),
                range.to(),
                range.to_virtual(),
            )
        });

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
