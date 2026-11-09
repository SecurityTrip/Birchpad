//! The lines of a document against its text in HEAD, as the margin marks them: added, modified,
//! and where lines were deleted. Line breaks do not count, so a file Git keeps with LF and the
//! document has with CRLF shows no change.

use std::ops::Range;

use birchpad_core::{ChangeSet, LineEnding, Rope, convert_line_breaks, diff_lines};

/// How a line differs from HEAD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GitChange {
    /// Lines HEAD does not have.
    Added,
    /// Lines in place of others of HEAD.
    Modified,
    /// HEAD has lines before this one that the document does not.
    Deleted,
}

/// Ranges of lines of the document, sorted, each with its change. A deletion is the line after
/// the lines deleted (the last line, when they were at the end), one line long.
pub type LineChanges = Vec<(Range<usize>, GitChange)>;

fn with_lf(text: &Rope) -> Rope {
    let edits = convert_line_breaks(text, LineEnding::Lf);
    if edits.is_empty() {
        return text.clone();
    }
    let mut converted = text.clone();
    if let Ok(changes) = ChangeSet::from_edits(text, edits) {
        changes.apply(&mut converted);
    }
    converted
}

/// The changes of the lines of `text` against `head`.
pub fn line_changes(head: &Rope, text: &Rope) -> LineChanges {
    let (head, text) = (with_lf(head), with_lf(text));
    let diff = diff_lines(&head, &text);
    let old_count = birchpad_core::motion::line_count(&head);
    let new_count = birchpad_core::motion::line_count(&text);
    let last = new_count.saturating_sub(1);
    let mut changes: LineChanges = Vec::new();
    let (mut old_at, mut new_at) = (0, 0);
    let mut runs = diff.equal.clone();
    runs.push((old_count, new_count, 0));
    for (old_start, new_start, count) in runs {
        let old_gap = old_start.saturating_sub(old_at);
        let new_gap = new_start.saturating_sub(new_at);
        if new_gap > 0 {
            let change = if old_gap > 0 {
                GitChange::Modified
            } else {
                GitChange::Added
            };
            changes.push((new_at..new_start, change));
        } else if old_gap > 0 {
            let line = new_at.min(last);
            changes.push((line..line + 1, GitChange::Deleted));
        }
        old_at = old_at.max(old_start + count);
        new_at = new_at.max(new_start + count);
    }
    // A deletion at the end lands on the last line, which may have changed itself.
    changes
        .dedup_by(|later, earlier| later.0.start < earlier.0.end && later.1 == GitChange::Deleted);
    changes
}

/// The first line of the next (or previous) block of changes after (before) `line`'s, around
/// the ends; `None` without changes.
pub fn next_change(changes: &LineChanges, line: usize, forward: bool) -> Option<usize> {
    let starts: Vec<usize> = changes.iter().map(|(lines, _)| lines.start).collect();
    if forward {
        starts
            .iter()
            .find(|start| **start > line)
            .or(starts.first())
            .copied()
    } else {
        // The block `line` is in counts as its own: the one before it is previous.
        let current = changes
            .iter()
            .find(|(lines, _)| lines.contains(&line))
            .map_or(line, |(lines, _)| lines.start);
        starts
            .iter()
            .rev()
            .find(|start| **start < current)
            .or(starts.last())
            .copied()
    }
}
