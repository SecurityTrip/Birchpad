//! Comparing two texts line by line, as Notepad++'s Compare plugin does: which lines one text
//! adds, which it removes and which it changes.

use std::ops::Range;
use std::time::{Duration, Instant};

use similar::{Algorithm, DiffOp};

/// How long a comparison may take before it settles for a coarser answer.
const DEADLINE: Duration = Duration::from_secs(2);

/// What a difference does to the old text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// Lines only the new text has.
    Added,
    /// Lines only the old text has.
    Removed,
    /// Lines of the old text that the new one has differently.
    Changed,
}

/// One difference: lines (0-based, end exclusive) of each text. An added block takes no lines of
/// the old text and a removed one none of the new: their empty range says where they would be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Difference {
    pub change: Change,
    pub old: Range<usize>,
    pub new: Range<usize>,
}

/// What lines are the same.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    /// Lines that differ only in spaces and tabs are the same.
    pub ignore_whitespace: bool,
    /// Lines that differ only in letter case are the same.
    pub ignore_case: bool,
}

/// The lines of `text` without their line breaks; a final line break does not start a line.
pub fn lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&str> = text
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect();
    if text.ends_with('\n') {
        lines.pop();
    }
    lines
}

fn key(line: &str, options: Options) -> String {
    let line: String = if options.ignore_whitespace {
        line.chars().filter(|c| *c != ' ' && *c != '\t').collect()
    } else {
        line.to_owned()
    };
    if options.ignore_case {
        line.to_lowercase()
    } else {
        line
    }
}

/// The differences between `old` and `new`, in order. Line breaks do not count: a text with
/// CRLF is the same as one with LF.
pub fn compare(old: &str, new: &str, options: Options) -> Vec<Difference> {
    let old: Vec<String> = lines(old).into_iter().map(|l| key(l, options)).collect();
    let new: Vec<String> = lines(new).into_iter().map(|l| key(l, options)).collect();
    let ops = similar::capture_diff_slices_deadline(
        Algorithm::Myers,
        &old,
        &new,
        Some(Instant::now() + DEADLINE),
    );
    let mut differences: Vec<Difference> = Vec::new();
    // Where each operation starts, from the lengths of the ones before it: the operations come in
    // order, but a deletion's `new_index` is not always where it is (similar 2.7 gave a deletion
    // at the start of the old text the new index 3 after an insertion of three lines).
    let (mut old_at, mut new_at) = (0, 0);
    for op in ops {
        let (change, old_len, new_len) = match op {
            DiffOp::Equal { len, .. } => {
                old_at += len;
                new_at += len;
                continue;
            }
            DiffOp::Delete { old_len, .. } => (Change::Removed, old_len, 0),
            DiffOp::Insert { new_len, .. } => (Change::Added, 0, new_len),
            DiffOp::Replace {
                old_len, new_len, ..
            } => (Change::Changed, old_len, new_len),
        };
        let old_range = old_at..old_at + old_len;
        let new_range = new_at..new_at + new_len;
        (old_at, new_at) = (old_range.end, new_range.end);
        // A removal next to an addition is a change.
        if let Some(last) = differences.last_mut()
            && last.old.end == old_range.start
            && last.new.end == new_range.start
        {
            last.change = Change::Changed;
            last.old.end = old_range.end;
            last.new.end = new_range.end;
            continue;
        }
        differences.push(Difference {
            change,
            old: old_range,
            new: new_range,
        });
    }
    differences
}

/// The difference after (or, `forward` false, before) `line` of the new text, wrapping around.
pub fn next(differences: &[Difference], line: usize, forward: bool) -> Option<&Difference> {
    if forward {
        differences
            .iter()
            .find(|difference| difference.new.start > line)
            .or_else(|| differences.first())
    } else {
        differences
            .iter()
            .rev()
            .find(|difference| difference.new.start < line)
            .or_else(|| differences.last())
    }
}
