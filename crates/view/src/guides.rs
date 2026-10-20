//! Indentation guides, as Scintilla draws them for Notepad++ (its "look both" mode): dotted
//! lines every indentation level inside the leading whitespace of a line, continued through
//! blank lines.

use birchpad_core::Rope;
use birchpad_core::motion::{line_count, line_range};

use crate::cells::cells_at;

/// How far a blank line looks up and down for a line with text, as in Scintilla.
const LOOK_AROUND: usize = 20;
/// Leading whitespace is measured up to this many characters; a line with more counts as text.
const MAX_INDENT_SCAN: usize = 4096;

/// The indentation of `line` in columns, and whether the line is blank (nothing but spaces and
/// tabs).
pub fn indentation(text: &Rope, line: usize, tab_width: usize) -> (usize, bool) {
    let mut column = 0;
    for (index, ch) in text.slice(line_range(text, line)).chars().enumerate() {
        if (ch != ' ' && ch != '\t') || index == MAX_INDENT_SCAN {
            return (column, false);
        }
        column += cells_at(ch, column, tab_width);
    }
    (column, true)
}

/// Columns of the indentation guides of `line`: one every `tab_width` columns inside its
/// indentation, none at column 0. A blank line takes the deepest indentation of the nearest
/// lines with text above (one level deeper if that line starts a fold) and below, so guides run
/// through blank lines inside blocks.
pub fn indent_guides(
    text: &Rope,
    line: usize,
    tab_width: usize,
    is_fold_header: impl Fn(usize) -> bool,
) -> impl Iterator<Item = usize> {
    let tab_width = tab_width.max(1);
    let (mut extent, blank) = indentation(text, line, tab_width);
    if blank {
        let with_text = |other: usize| {
            let (indent, blank) = indentation(text, other, tab_width);
            (!blank).then_some(indent)
        };
        let above = (line.saturating_sub(LOOK_AROUND)..line)
            .rev()
            .find_map(|other| {
                with_text(other)
                    .map(|indent| indent + if is_fold_header(other) { tab_width } else { 0 })
            });
        let below = (line + 1..(line + 1 + LOOK_AROUND).min(line_count(text))).find_map(with_text);
        extent = extent.max(above.unwrap_or(0)).max(below.unwrap_or(0));
    }
    (tab_width..extent).step_by(tab_width)
}
