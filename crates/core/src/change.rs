//! Change sets: a compact description of how one version of a text becomes the next.
//!
//! A [`ChangeSet`] walks the old text from start to end with three operations: keep bytes,
//! delete bytes, insert a string. That form makes the three things an editor needs cheap:
//! applying the change, inverting it (undo), composing two changes (grouping typing into one
//! undo step) and mapping positions through it (moving other views' cursors, bookmarks, marks).

use std::ops::Range;

use ropey::{Rope, RopeBuilder};

/// Above this many operations, [`ChangeSet::apply`] rebuilds the rope in one pass instead of
/// editing it in place: one O(n) pass beats a million O(log n) edits (Replace All, EOL conversion).
const REBUILD_THRESHOLD: usize = 4096;

/// A single edit expressed in coordinates of the text *before* any edit of the batch is applied.
///
/// An empty `range` is a pure insertion; an empty `text` is a pure deletion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
}

impl Edit {
    pub fn insert(pos: usize, text: impl Into<String>) -> Self {
        Self {
            range: pos..pos,
            text: text.into(),
        }
    }

    pub fn delete(range: Range<usize>) -> Self {
        Self {
            range,
            text: String::new(),
        }
    }

    pub fn replace(range: Range<usize>, text: impl Into<String>) -> Self {
        Self {
            range,
            text: text.into(),
        }
    }
}

/// Why a batch of edits was rejected by [`ChangeSet::from_edits`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidEdit {
    #[error("edit range {start}..{end} is reversed")]
    Reversed { start: usize, end: usize },
    #[error("edit range ends at {end}, past the end of the text ({len} bytes)")]
    OutOfBounds { end: usize, len: usize },
    #[error("edit boundary {pos} is not on a character boundary")]
    NotCharBoundary { pos: usize },
    #[error("edit starting at {start} overlaps or precedes the previous edit ending at {prev_end}")]
    Unordered { start: usize, prev_end: usize },
}

/// One step of a [`ChangeSet`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    /// Keep the next `n` bytes of the old text.
    Retain(usize),
    /// Remove the next `n` bytes of the old text.
    Delete(usize),
    /// Insert a string at the current position.
    Insert(String),
}

/// Which side of an insertion a position sticks to when the insertion happens exactly at it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assoc {
    /// Stay before the inserted text (Scintilla's behavior for carets and markers).
    Before,
    /// Move after the inserted text.
    After,
}

/// A transformation from a text of [`len_before`](Self::len_before) bytes to a text of
/// [`len_after`](Self::len_after) bytes.
///
/// Operations are kept in canonical form: no empty operations, no two adjacent operations of the
/// same kind, and an insertion always precedes an adjacent deletion. Two change sets that do the
/// same thing therefore compare equal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeSet {
    ops: Vec<Operation>,
    len_before: usize,
    len_after: usize,
}

impl ChangeSet {
    /// A change set that leaves a text of `len` bytes untouched.
    pub fn identity(len: usize) -> Self {
        let mut changes = Self::empty();
        changes.retain(len);
        changes
    }

    /// Builds a change set from edits sorted by position that do not overlap.
    ///
    /// Edits may touch: an insertion at the end of a previous edit's range is allowed, and
    /// several insertions at the same position are applied in order.
    pub fn from_edits<I>(text: &Rope, edits: I) -> Result<Self, InvalidEdit>
    where
        I: IntoIterator<Item = Edit>,
    {
        let len = text.len();
        let mut changes = Self::empty();
        let mut pos = 0;

        for Edit {
            range,
            text: insert,
        } in edits
        {
            let Range { start, end } = range;
            if start > end {
                return Err(InvalidEdit::Reversed { start, end });
            }
            if end > len {
                return Err(InvalidEdit::OutOfBounds { end, len });
            }
            if start < pos {
                return Err(InvalidEdit::Unordered {
                    start,
                    prev_end: pos,
                });
            }
            for boundary in [start, end] {
                if !text.is_char_boundary(boundary) {
                    return Err(InvalidEdit::NotCharBoundary { pos: boundary });
                }
            }

            changes.retain(start - pos);
            changes.delete(end - start);
            changes.insert(insert);
            pos = end;
        }
        changes.retain(len - pos);
        Ok(changes)
    }

    pub fn len_before(&self) -> usize {
        self.len_before
    }

    pub fn len_after(&self) -> usize {
        self.len_after
    }

    pub fn ops(&self) -> &[Operation] {
        &self.ops
    }

    /// True if applying the change set leaves the text unchanged.
    pub fn is_identity(&self) -> bool {
        self.ops.iter().all(|op| matches!(op, Operation::Retain(_)))
    }

    /// Applies the change set to `text`.
    ///
    /// # Panics
    ///
    /// Panics if `text` is not exactly [`len_before`](Self::len_before) bytes long, i.e. the
    /// change set was built for a different version of the text.
    pub fn apply(&self, text: &mut Rope) {
        assert_eq!(
            text.len(),
            self.len_before,
            "change set was built for a text of a different length"
        );
        if self.ops.len() > REBUILD_THRESHOLD {
            *text = self.rebuild(text);
            return;
        }
        let mut pos = 0;
        for op in &self.ops {
            match op {
                Operation::Retain(n) => pos += n,
                Operation::Delete(n) => text.remove(pos..pos + n),
                Operation::Insert(s) => {
                    text.insert(pos, s);
                    pos += s.len();
                }
            }
        }
    }

    /// Builds the changed text from scratch, streaming the retained parts of `text`.
    fn rebuild(&self, text: &Rope) -> Rope {
        let mut builder = RopeBuilder::new();
        let mut pos = 0;
        for op in &self.ops {
            match op {
                Operation::Retain(n) => {
                    for chunk in text.slice(pos..pos + n).chunks() {
                        builder.append(chunk);
                    }
                    pos += n;
                }
                Operation::Delete(n) => pos += n,
                Operation::Insert(s) => builder.append(s),
            }
        }
        builder.finish()
    }

    /// Returns the change set that undoes `self`. `original` is the text `self` applies to.
    pub fn invert(&self, original: &Rope) -> Self {
        assert_eq!(original.len(), self.len_before);
        let mut inverse = Self::empty();
        let mut pos = 0;
        for op in &self.ops {
            match op {
                Operation::Retain(n) => {
                    inverse.retain(*n);
                    pos += n;
                }
                Operation::Delete(n) => {
                    inverse.insert(original.slice(pos..pos + n).to_string());
                    pos += n;
                }
                Operation::Insert(s) => inverse.delete(s.len()),
            }
        }
        inverse
    }

    /// Combines `self` followed by `next` into a single change set.
    ///
    /// # Panics
    ///
    /// Panics if `next` does not apply to the result of `self`.
    pub fn compose(self, next: Self) -> Self {
        assert_eq!(
            self.len_after, next.len_before,
            "composed change sets do not line up"
        );
        let mut out = Self::empty();
        let mut first = self.ops.into_iter();
        let mut second = next.ops.into_iter();
        let mut a = first.next();
        let mut b = second.next();

        loop {
            match (a.take(), b.take()) {
                (None, None) => break,
                // Text deleted by the first change is never seen by the second.
                (Some(Operation::Delete(n)), rest) => {
                    out.delete(n);
                    a = first.next();
                    b = rest;
                }
                // Text inserted by the second change is new.
                (rest, Some(Operation::Insert(s))) => {
                    out.insert(s);
                    a = rest;
                    b = second.next();
                }
                (Some(Operation::Retain(i)), Some(Operation::Retain(j))) => {
                    let n = i.min(j);
                    out.retain(n);
                    a = remainder(Operation::Retain, i - n).or_else(|| first.next());
                    b = remainder(Operation::Retain, j - n).or_else(|| second.next());
                }
                (Some(Operation::Retain(i)), Some(Operation::Delete(j))) => {
                    let n = i.min(j);
                    out.delete(n);
                    a = remainder(Operation::Retain, i - n).or_else(|| first.next());
                    b = remainder(Operation::Delete, j - n).or_else(|| second.next());
                }
                (Some(Operation::Insert(s)), Some(Operation::Retain(j))) => {
                    if s.len() <= j {
                        let n = s.len();
                        out.insert(s);
                        a = first.next();
                        b = remainder(Operation::Retain, j - n).or_else(|| second.next());
                    } else {
                        // Positions of `next` are char boundaries of the intermediate text, so
                        // `j` is a char boundary inside `s`.
                        out.insert(s[..j].to_owned());
                        a = Some(Operation::Insert(s[j..].to_owned()));
                        b = second.next();
                    }
                }
                // The second change deletes (part of) what the first one inserted.
                (Some(Operation::Insert(s)), Some(Operation::Delete(j))) => {
                    if s.len() <= j {
                        a = first.next();
                        b = remainder(Operation::Delete, j - s.len()).or_else(|| second.next());
                    } else {
                        a = Some(Operation::Insert(s[j..].to_owned()));
                        b = second.next();
                    }
                }
                (None, Some(_)) | (Some(_), None) => {
                    unreachable!("lengths were checked to line up")
                }
            }
        }
        out
    }

    /// Maps a position in the old text to the corresponding position in the new text.
    ///
    /// A position inside a deleted range collapses to where the deletion happened (after any
    /// text inserted in its place).
    pub fn map_pos(&self, pos: usize, assoc: Assoc) -> usize {
        let mut old = 0;
        let mut new = 0;
        for op in &self.ops {
            match op {
                Operation::Retain(n) => {
                    if pos < old + n {
                        return new + (pos - old);
                    }
                    old += n;
                    new += n;
                }
                Operation::Delete(n) => {
                    if pos < old + n {
                        return new;
                    }
                    old += n;
                }
                Operation::Insert(s) => {
                    if pos == old && assoc == Assoc::Before {
                        return new;
                    }
                    new += s.len();
                }
            }
        }
        new + pos.saturating_sub(old)
    }

    fn empty() -> Self {
        Self {
            ops: Vec::new(),
            len_before: 0,
            len_after: 0,
        }
    }

    fn retain(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        self.len_before += n;
        self.len_after += n;
        if let Some(Operation::Retain(last)) = self.ops.last_mut() {
            *last += n;
        } else {
            self.ops.push(Operation::Retain(n));
        }
    }

    fn delete(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        self.len_before += n;
        if let Some(Operation::Delete(last)) = self.ops.last_mut() {
            *last += n;
        } else {
            self.ops.push(Operation::Delete(n));
        }
    }

    fn insert(&mut self, text: String) {
        if text.is_empty() {
            return;
        }
        self.len_after += text.len();
        let len = self.ops.len();
        match self.ops.as_mut_slice() {
            [.., Operation::Insert(last)] => last.push_str(&text),
            // Keep insertions in front of adjacent deletions.
            [.., Operation::Insert(prev), Operation::Delete(_)] => prev.push_str(&text),
            [.., Operation::Delete(_)] => self.ops.insert(len - 1, Operation::Insert(text)),
            _ => self.ops.push(Operation::Insert(text)),
        }
    }
}

fn remainder(op: fn(usize) -> Operation, n: usize) -> Option<Operation> {
    (n > 0).then(|| op(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(text: &str, edits: Vec<Edit>) -> (ChangeSet, String) {
        let mut rope = Rope::from_str(text);
        let changes = ChangeSet::from_edits(&rope, edits).unwrap();
        changes.apply(&mut rope);
        (changes, rope.to_string())
    }

    #[test]
    fn applies_multiple_edits() {
        let (_, result) = apply(
            "hello world",
            vec![Edit::replace(0..5, "goodbye"), Edit::insert(11, "!")],
        );
        assert_eq!(result, "goodbye world!");
    }

    #[test]
    fn canonical_form_puts_insert_before_delete() {
        let (changes, _) = apply("abc", vec![Edit::replace(1..2, "XY")]);
        assert_eq!(
            changes.ops(),
            [
                Operation::Retain(1),
                Operation::Insert("XY".into()),
                Operation::Delete(1),
                Operation::Retain(1),
            ]
        );
    }

    #[test]
    fn rejects_bad_edits() {
        let rope = Rope::from_str("жук");
        let err = |edits: Vec<Edit>| ChangeSet::from_edits(&rope, edits).unwrap_err();
        assert_eq!(
            err(vec![Edit::delete(1..2)]),
            InvalidEdit::NotCharBoundary { pos: 1 }
        );
        assert_eq!(
            err(vec![Edit::delete(0..99)]),
            InvalidEdit::OutOfBounds { end: 99, len: 6 }
        );
        assert_eq!(
            err(vec![Edit::delete(2..4), Edit::delete(0..2)]),
            InvalidEdit::Unordered {
                start: 0,
                prev_end: 4
            }
        );
    }

    #[test]
    fn invert_restores_original() {
        // Cyrillic letters take two bytes each: "два" is bytes 9..15, the text ends at 22.
        let original = Rope::from_str("один два три");
        let changes = ChangeSet::from_edits(
            &original,
            vec![Edit::replace(9..15, "2"), Edit::insert(22, ".")],
        )
        .unwrap();
        let mut text = original.clone();
        changes.apply(&mut text);
        assert_eq!(text, "один 2 три.");
        changes.invert(&original).apply(&mut text);
        assert_eq!(text, original);
    }

    #[test]
    fn compose_equals_sequential_application() {
        let mut text = Rope::from_str("abc");
        let first = ChangeSet::from_edits(&text, vec![Edit::insert(1, "123")]).unwrap();
        let mut intermediate = text.clone();
        first.apply(&mut intermediate);
        // Deletes part of the inserted text and part of the original.
        let second = ChangeSet::from_edits(&intermediate, vec![Edit::delete(2..5)]).unwrap();
        first.compose(second).apply(&mut text);
        assert_eq!(text, "a1c");
    }

    #[test]
    fn many_edits_rebuild_the_same_text() {
        let line = "ab\r\n";
        let original = Rope::from_str(&line.repeat(REBUILD_THRESHOLD + 10));
        let edits = (0..REBUILD_THRESHOLD + 10).map(|i| Edit::replace(i * 4 + 2..i * 4 + 4, "\n"));
        let changes = ChangeSet::from_edits(&original, edits).unwrap();
        assert!(changes.ops().len() > REBUILD_THRESHOLD);
        let mut text = original.clone();
        changes.apply(&mut text);
        assert_eq!(text, "ab\n".repeat(REBUILD_THRESHOLD + 10).as_str());
        changes.invert(&original).apply(&mut text);
        assert_eq!(text, original);
    }

    #[test]
    fn map_pos_respects_assoc() {
        let rope = Rope::from_str("abcdef");
        let changes =
            ChangeSet::from_edits(&rope, vec![Edit::insert(2, "XX"), Edit::delete(3..5)]).unwrap();
        assert_eq!(changes.map_pos(1, Assoc::Before), 1);
        assert_eq!(changes.map_pos(2, Assoc::Before), 2);
        assert_eq!(changes.map_pos(2, Assoc::After), 4);
        assert_eq!(changes.map_pos(4, Assoc::Before), 5, "inside deletion");
        assert_eq!(changes.map_pos(6, Assoc::Before), 6, "end of text");
    }
}
