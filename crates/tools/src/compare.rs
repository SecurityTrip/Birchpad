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
    for op in ops {
        let (change, old_range, new_range) = match op {
            DiffOp::Equal { .. } => continue,
            DiffOp::Delete {
                old_index,
                old_len,
                new_index,
            } => (
                Change::Removed,
                old_index..old_index + old_len,
                new_index..new_index,
            ),
            DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => (
                Change::Added,
                old_index..old_index,
                new_index..new_index + new_len,
            ),
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => (
                Change::Changed,
                old_index..old_index + old_len,
                new_index..new_index + new_len,
            ),
        };
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

#[cfg(test)]
mod tests {
    use super::*;

    fn diff(old: &str, new: &str) -> Vec<(Change, Range<usize>, Range<usize>)> {
        compare(old, new, Options::default())
            .into_iter()
            .map(|d| (d.change, d.old, d.new))
            .collect()
    }

    #[test]
    fn finds_added_removed_and_changed_lines() {
        assert_eq!(
            diff("a\nb\nc\nd\n", "a\nx\nc\nd\ne\n"),
            [(Change::Changed, 1..2, 1..2), (Change::Added, 4..4, 4..5)]
        );
        assert_eq!(diff("a\nb\nc", "a\nc"), [(Change::Removed, 1..2, 1..1)]);
        assert_eq!(diff("b\nc", "a\nb\nc"), [(Change::Added, 0..0, 0..1)]);
    }

    #[test]
    fn same_texts_have_no_differences() {
        assert!(diff("", "").is_empty());
        assert!(diff("a\r\nb\r\n", "a\nb\n").is_empty());
        assert!(diff("x", "x").is_empty());
    }

    #[test]
    fn an_empty_text_against_another() {
        assert_eq!(diff("", "a\nb"), [(Change::Added, 0..0, 0..2)]);
        assert_eq!(diff("a\nb\n", ""), [(Change::Removed, 0..2, 0..0)]);
        // A final line break alone makes no line.
        assert_eq!(diff("a", "a\n"), []);
        assert_eq!(diff("a\n", "a\n\n"), [(Change::Added, 1..1, 1..2)]);
    }

    #[test]
    fn can_ignore_whitespace_and_case() {
        let old = "if (a)\n\treturn B;\n";
        let new = "if ( a )\n    return b;\n";
        assert_eq!(compare(old, new, Options::default()).len(), 1);
        let loose = Options {
            ignore_whitespace: true,
            ignore_case: false,
        };
        assert_eq!(compare(old, new, loose).len(), 1);
        let looser = Options {
            ignore_whitespace: true,
            ignore_case: true,
        };
        assert!(compare(old, new, looser).is_empty());
    }

    #[test]
    fn moves_between_differences_wrapping_around() {
        let differences = compare("a\nb\nc\nd\ne", "a\nB\nc\nD\ne", Options::default());
        assert_eq!(differences.len(), 2);
        assert_eq!(next(&differences, 0, true).unwrap().new, 1..2);
        assert_eq!(next(&differences, 1, true).unwrap().new, 3..4);
        assert_eq!(next(&differences, 3, true).unwrap().new, 1..2);
        assert_eq!(next(&differences, 4, false).unwrap().new, 3..4);
        assert_eq!(next(&differences, 1, false).unwrap().new, 3..4);
        assert_eq!(next(&[], 0, true), None);
        assert_eq!(next(&[], 0, false), None);
    }

    #[test]
    fn splits_lines_without_their_breaks() {
        assert_eq!(lines(""), Vec::<&str>::new());
        assert_eq!(lines("\n"), [""]);
        assert_eq!(lines("a\r\nb"), ["a", "b"]);
        assert_eq!(lines("a\rb"), ["a\rb"]);
    }
}
