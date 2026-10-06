//! Change history, as in Notepad++: the lines changed since a document was opened.
//!
//! A line is *modified* while it differs from the text as last saved, and a *saved* change once
//! a save took it in: it matches the saved text but not the text as opened. Both come from
//! comparing lines rather than from the edits that made them, so undoing back to the saved text
//! clears the modified lines.

use std::hash::{DefaultHasher, Hasher};
use std::ops::Range;
use std::sync::Arc;
use std::time::{Duration, Instant};

use similar::{Algorithm, DiffOp};

use crate::Rope;
use crate::motion::{LINE_TYPE, line_count, line_of};

/// How a line changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineChange {
    /// Differs from the text as last saved: Notepad++'s orange.
    Modified,
    /// Changed since the document was opened, and saved: Notepad++'s green.
    Saved,
}

/// How long a comparison looks for the smallest difference before it settles for a larger one,
/// so that two very different texts cannot take long.
const DIFF_DEADLINE: Duration = Duration::from_millis(200);

/// How the lines of a text differ from those of an earlier one.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LineDiff {
    /// Sorted, disjoint ranges of the lines of the new text that are not in the old one.
    /// Where lines were only deleted, the line that took their place counts as changed.
    pub changed: Vec<Range<usize>>,
    /// Runs of lines both texts have, in order: the first line in the old text, the first
    /// line in the new text, and the number of lines.
    pub equal: Vec<(usize, usize, usize)>,
}

/// Compares the lines of `old` and `new`, line breaks included.
///
/// The lines both texts start and end with are compared directly; only the lines between them
/// are diffed, so a small edit of a large text costs little more than reading it.
pub fn diff_lines(old: &Rope, new: &Rope) -> LineDiff {
    let (old_count, new_count) = (line_count(old), line_count(new));
    let common = old_count.min(new_count);
    // The bytes both texts start with hold the lines before the one with their last byte
    // (that line may end differently: a CR followed by LF in one text only). Comparing lines
    // goes on from there.
    let bytes = common_prefix(old, new);
    let mut prefix = if bytes == 0 {
        0
    } else {
        line_of(old, bytes - 1).min(common)
    };
    let (mut a, mut b) = (
        old.lines_at(prefix, LINE_TYPE),
        new.lines_at(prefix, LINE_TYPE),
    );
    while prefix < common && a.next() == b.next() {
        prefix += 1;
    }
    // Likewise the bytes both end with hold the lines after the one with their first byte.
    let bytes = common_suffix(old, new, old.len().min(new.len()) - bytes);
    let mut suffix = if bytes == 0 {
        0
    } else {
        (old_count - 1 - line_of(old, old.len() - bytes)).min(common - prefix)
    };
    let (mut a, mut b) = (
        old.lines_at(old_count - suffix, LINE_TYPE),
        new.lines_at(new_count - suffix, LINE_TYPE),
    );
    while suffix < common - prefix && a.prev() == b.prev() {
        suffix += 1;
    }

    let mut diff = LineDiff::default();
    if prefix > 0 {
        diff.equal.push((0, 0, prefix));
    }
    let old_middle = prefix..old_count - suffix;
    let new_middle = prefix..new_count - suffix;
    if !old_middle.is_empty() || !new_middle.is_empty() {
        let ops = similar::capture_diff_slices_deadline(
            Algorithm::Myers,
            &line_hashes(old, old_middle),
            &line_hashes(new, new_middle),
            Some(Instant::now() + DIFF_DEADLINE),
        );
        let last = new_count - 1;
        for op in ops {
            match op {
                DiffOp::Equal {
                    old_index,
                    new_index,
                    len,
                } => diff
                    .equal
                    .push((old_index + prefix, new_index + prefix, len)),
                DiffOp::Delete { new_index, .. } => {
                    let line = (new_index + prefix).min(last);
                    diff.changed.push(line..line + 1);
                }
                DiffOp::Insert {
                    new_index, new_len, ..
                }
                | DiffOp::Replace {
                    new_index, new_len, ..
                } => {
                    let start = new_index + prefix;
                    diff.changed.push(start..start + new_len);
                }
            }
        }
    }
    if suffix > 0 {
        diff.equal
            .push((old_count - suffix, new_count - suffix, suffix));
    }
    diff.changed = merge(diff.changed);
    diff
}

/// The number of bytes `a` and `b` start with in common.
fn common_prefix(a: &Rope, b: &Rope) -> usize {
    let (mut chunks_a, mut chunks_b) = (a.chunks(), b.chunks());
    let (mut x, mut y): (&[u8], &[u8]) = (&[], &[]);
    let mut equal = 0;
    loop {
        if x.is_empty() {
            match chunks_a.next() {
                Some(chunk) => x = chunk.as_bytes(),
                None => return equal,
            }
        }
        if y.is_empty() {
            match chunks_b.next() {
                Some(chunk) => y = chunk.as_bytes(),
                None => return equal,
            }
        }
        let n = x.len().min(y.len());
        if x[..n] != y[..n] {
            return equal + x.iter().zip(y).take_while(|(p, q)| p == q).count();
        }
        equal += n;
        x = &x[n..];
        y = &y[n..];
    }
}

/// The number of bytes `a` and `b` end with in common, at most `limit`.
fn common_suffix(a: &Rope, b: &Rope, limit: usize) -> usize {
    let (mut chunks_a, mut chunks_b) = (a.chunks_at(a.len()).0, b.chunks_at(b.len()).0);
    let (mut x, mut y): (&[u8], &[u8]) = (&[], &[]);
    let mut equal = 0;
    while equal < limit {
        if x.is_empty() {
            match chunks_a.prev() {
                Some(chunk) => x = chunk.as_bytes(),
                None => break,
            }
        }
        if y.is_empty() {
            match chunks_b.prev() {
                Some(chunk) => y = chunk.as_bytes(),
                None => break,
            }
        }
        let n = x.len().min(y.len());
        let (tail_x, tail_y) = (&x[x.len() - n..], &y[y.len() - n..]);
        if tail_x != tail_y {
            let same = tail_x
                .iter()
                .rev()
                .zip(tail_y.iter().rev())
                .take_while(|(p, q)| p == q)
                .count();
            equal += same;
            break;
        }
        equal += n;
        x = &x[..x.len() - n];
        y = &y[..y.len() - n];
    }
    equal.min(limit)
}

/// A hash of each of `lines` of `text`.
fn line_hashes(text: &Rope, lines: Range<usize>) -> Vec<u64> {
    let mut iter = text.lines_at(lines.start, LINE_TYPE);
    lines
        .map(|_| {
            let line = iter.next().expect("the line exists");
            let mut hasher = DefaultHasher::new();
            for chunk in line.chunks() {
                hasher.write(chunk.as_bytes());
            }
            hasher.finish()
        })
        .collect()
}

/// Sorts `ranges` and joins those that overlap or touch.
fn merge(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.sort_unstable_by_key(|range| range.start);
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match merged.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => merged.push(range),
        }
    }
    merged
}

/// The texts a document's change history compares with: as opened and as last saved.
///
/// Cloning it is cheap (ropes share their text), so the comparison can run in the background.
#[derive(Debug, Clone)]
pub struct ChangeHistory {
    original: Rope,
    saved: Rope,
    /// Lines of `saved` that differ from `original`, once compared.
    saved_changes: Option<Arc<[Range<usize>]>>,
}

impl ChangeHistory {
    /// A history that starts with `text`: nothing has changed yet.
    pub fn new(text: Rope) -> Self {
        Self {
            original: text.clone(),
            saved: text,
            saved_changes: Some(Arc::from([])),
        }
    }

    /// The document was saved as `text`.
    pub fn save(&mut self, text: Rope) {
        self.saved = text;
        self.saved_changes = None;
    }

    /// `text` was added at the end of the file by another program (View > Monitoring): it is
    /// part of the file, not a change.
    pub fn append(&mut self, text: &str) {
        let original_len = self.original.len();
        self.original.insert(original_len, text);
        let saved_len = self.saved.len();
        self.saved.insert(saved_len, text);
        self.saved_changes = None;
    }

    /// The changed lines of `text`, the document's current text, as sorted, disjoint ranges.
    pub fn lines(&mut self, text: &Rope) -> Vec<(Range<usize>, LineChange)> {
        let saved_changes = self
            .saved_changes
            .get_or_insert_with(|| diff_lines(&self.original, &self.saved).changed.into())
            .clone();
        let diff = diff_lines(&self.saved, text);
        // Saved changes on lines the current text still has, moved to their place in it.
        let mut saved = Vec::new();
        for &(old, new, len) in &diff.equal {
            let first = saved_changes.partition_point(|range| range.end <= old);
            for range in saved_changes[first..]
                .iter()
                .take_while(|range| range.start < old + len)
            {
                let start = range.start.max(old) - old + new;
                let end = range.end.min(old + len) - old + new;
                saved.push(start..end);
            }
        }
        // A line that is modified again is modified.
        let saved = subtract(merge(saved), &diff.changed);
        let mut lines: Vec<_> = diff
            .changed
            .into_iter()
            .map(|range| (range, LineChange::Modified))
            .chain(saved.into_iter().map(|range| (range, LineChange::Saved)))
            .collect();
        lines.sort_unstable_by_key(|(range, _)| range.start);
        lines
    }
}

/// `ranges` without the lines in `holes`; both sorted and disjoint.
fn subtract(ranges: Vec<Range<usize>>, holes: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut result = Vec::with_capacity(ranges.len());
    let mut hole = 0;
    for mut range in ranges {
        while hole < holes.len() && holes[hole].end <= range.start {
            hole += 1;
        }
        let mut next = hole;
        while next < holes.len() && holes[next].start < range.end {
            if holes[next].start > range.start {
                result.push(range.start..holes[next].start);
            }
            range.start = range.start.max(holes[next].end);
            next += 1;
        }
        if range.start < range.end {
            result.push(range);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::single_range_in_vec_init,
        reason = "one range of lines is the expected result"
    )]

    use proptest::prelude::*;

    use super::*;

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
    fn subtract_cuts_holes() {
        assert_eq!(subtract(vec![0..10], &[2..3, 5..7]), [0..2, 3..5, 7..10]);
        assert_eq!(subtract(vec![0..2, 4..6], &[1..5]), [0..1, 5..6]);
        assert_eq!(subtract(vec![0..2], &[0..2]), Vec::<Range<usize>>::new());
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
        eprintln!("one edit in 10 MB: {:?}", started.elapsed());
        assert_eq!(lines, [(125_000..125_001, Modified)]);

        // Edits at both ends: the whole text is compared.
        edited.insert(0, "x");
        edited.insert(edited.len(), "x");
        let started = Instant::now();
        let lines = history.lines(&edited);
        eprintln!("edits at both ends of 10 MB: {:?}", started.elapsed());
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
            prop_assert_eq!(merge(modified), diff_lines(&rope(&saved), &current).changed);
        }
    }
}
