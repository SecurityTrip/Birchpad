//! Comparing two texts line by line, as Notepad++'s Compare plugin does: which lines one text
//! adds, which it removes and which it changes; as ComparePlus does, the blank rows that keep
//! the two side by side ([`padding`]) and what differs within the changed lines ([`inline`]).

use std::ops::Range;
use std::time::{Duration, Instant};

use similar::{Algorithm, DiffOp};

/// How long a comparison may take before it settles for a coarser answer.
const DEADLINE: Duration = Duration::from_secs(2);
/// Lines longer than this, in characters, are not compared within: the whole line is marked.
pub const MAX_INLINE: usize = 10_000;

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

/// The blank rows each side needs so that the lines both have stay side by side: for the old
/// text and the new one, `(line, rows)` with the rows above `line` (a line past the last one:
/// below the text). Where one side has more lines in a difference, the other gets that many
/// rows after its own lines of it.
pub fn padding(differences: &[Difference]) -> [Vec<(usize, u32)>; 2] {
    let mut sides = [Vec::new(), Vec::new()];
    for difference in differences {
        let (old, new) = (difference.old.len(), difference.new.len());
        if old > new {
            sides[1].push((difference.new.end, (old - new) as u32));
        } else if new > old {
            sides[0].push((difference.old.end, (new - old) as u32));
        }
    }
    sides
}

/// The byte ranges of the old line and of the new one that differ.
pub type InlineRanges = (Vec<Range<usize>>, Vec<Range<usize>>);

/// What differs within a changed line: the byte ranges of `old` and of `new` that the other
/// does not have, character by character, as `options` sees them (spaces left out, letter case
/// ignored). `None` when the lines have no character in common where they could match, or one
/// is longer than [`MAX_INLINE`] characters: then the whole line is the difference.
pub fn inline(old: &str, new: &str, options: Options) -> Option<InlineRanges> {
    let chars = |line: &str| -> Option<Vec<(char, Range<usize>)>> {
        if line.chars().count() > MAX_INLINE {
            return None;
        }
        Some(
            line.char_indices()
                .filter(|(_, c)| !(options.ignore_whitespace && (*c == ' ' || *c == '\t')))
                .map(|(at, c)| {
                    let key = if options.ignore_case {
                        c.to_lowercase().next().unwrap_or(c)
                    } else {
                        c
                    };
                    (key, at..at + c.len_utf8())
                })
                .collect(),
        )
    };
    let (old_chars, new_chars) = (chars(old)?, chars(new)?);
    let old_keys: Vec<char> = old_chars.iter().map(|(key, _)| *key).collect();
    let new_keys: Vec<char> = new_chars.iter().map(|(key, _)| *key).collect();
    let ops = similar::capture_diff_slices_deadline(
        Algorithm::Myers,
        &old_keys,
        &new_keys,
        Some(Instant::now() + DEADLINE),
    );
    if !ops.iter().any(|op| matches!(op, DiffOp::Equal { .. })) {
        return None;
    }
    let mut sides: (Vec<Range<usize>>, Vec<Range<usize>>) = (Vec::new(), Vec::new());
    let push =
        |ranges: &mut Vec<Range<usize>>, chars: &[(char, Range<usize>)], at: usize, len: usize| {
            if len == 0 {
                return;
            }
            let range = chars[at].1.start..chars[at + len - 1].1.end;
            match ranges.last_mut() {
                Some(last) if last.end == range.start => last.end = range.end,
                _ => ranges.push(range),
            }
        };
    for op in ops {
        match op {
            DiffOp::Equal { .. } => {}
            DiffOp::Delete {
                old_index, old_len, ..
            } => push(&mut sides.0, &old_chars, old_index, old_len),
            DiffOp::Insert {
                new_index, new_len, ..
            } => push(&mut sides.1, &new_chars, new_index, new_len),
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                push(&mut sides.0, &old_chars, old_index, old_len);
                push(&mut sides.1, &new_chars, new_index, new_len);
            }
        }
    }
    Some(sides)
}

/// Where each line of `text` starts, in bytes, as [`lines`] splits it.
fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(at, _)| at + 1))
        .collect()
}

/// What differs within the changed lines of `differences`, for the old text and the new one, as
/// byte ranges of each text. The lines of a change are paired in order, the first with the
/// first; lines without a partner are different whole.
pub fn inline_differences(
    old: &str,
    new: &str,
    differences: &[Difference],
    options: Options,
) -> [Vec<Range<usize>>; 2] {
    let (old_lines, new_lines) = (lines(old), lines(new));
    let (old_starts, new_starts) = (line_starts(old), line_starts(new));
    let mut sides = [Vec::new(), Vec::new()];
    for difference in differences {
        if difference.change != Change::Changed {
            continue;
        }
        for (old_line, new_line) in difference.old.clone().zip(difference.new.clone()) {
            let (Some(old_text), Some(new_text)) =
                (old_lines.get(old_line), new_lines.get(new_line))
            else {
                continue;
            };
            let Some((old_ranges, new_ranges)) = inline(old_text, new_text, options) else {
                continue;
            };
            let (old_at, new_at) = (old_starts[old_line], new_starts[new_line]);
            sides[0].extend(
                old_ranges
                    .into_iter()
                    .map(|r| r.start + old_at..r.end + old_at),
            );
            sides[1].extend(
                new_ranges
                    .into_iter()
                    .map(|r| r.start + new_at..r.end + new_at),
            );
        }
    }
    sides
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
