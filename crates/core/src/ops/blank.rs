//! Edit > Blank Operations: trimming, joining lines with spaces, converting between tabs and
//! spaces. They work on the selected lines, or on the whole document.

use ropey::Rope;

use super::{line_texts, replace_block, target_lines};
use crate::line_ending::LineEnding;
use crate::selection::Selection;
use crate::transaction::Transaction;

/// Which blanks Trim removes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trim {
    Trailing,
    Leading,
    Both,
}

const BLANKS: [char; 2] = [' ', '\t'];

/// Trim Trailing Space, Trim Leading Space, Trim Leading and Trailing Space.
pub fn trim(
    text: &Rope,
    selection: &Selection,
    which: Trim,
    eol: LineEnding,
) -> Option<Transaction> {
    let lines = target_lines(text, selection);
    let trimmed: Vec<String> = line_texts(text, lines.clone())
        .iter()
        .map(|line| match which {
            Trim::Trailing => line.trim_end_matches(BLANKS),
            Trim::Leading => line.trim_start_matches(BLANKS),
            Trim::Both => line.trim_matches(BLANKS),
        })
        .map(str::to_owned)
        .collect();
    replace_block(text, selection, lines, &trimmed, eol)
}

/// EOL to Space: the lines become one, separated by spaces. With `trim` (Notepad++'s "Remove
/// Unnecessary Blank and EOL"), each line is trimmed first and empty ones are dropped.
pub fn eol_to_space(
    text: &Rope,
    selection: &Selection,
    trim: bool,
    eol: LineEnding,
) -> Option<Transaction> {
    let lines = target_lines(text, selection);
    if lines.len() < 2 && !trim {
        return None;
    }
    let contents = line_texts(text, lines.clone());
    let joined = if trim {
        contents
            .iter()
            .map(|line| line.trim_matches(BLANKS))
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        contents.join(" ")
    };
    replace_block(text, selection, lines, &[joined], eol)
}

/// TAB to Space: every tab becomes the spaces up to the next tab stop.
pub fn tabs_to_spaces(
    text: &Rope,
    selection: &Selection,
    tab_width: usize,
    eol: LineEnding,
) -> Option<Transaction> {
    let tab_width = tab_width.max(1);
    let lines = target_lines(text, selection);
    let converted: Vec<String> = line_texts(text, lines.clone())
        .iter()
        .map(|line| {
            let mut out = String::with_capacity(line.len());
            let mut column = 0;
            for ch in line.chars() {
                if ch == '\t' {
                    let spaces = tab_width - column % tab_width;
                    out.extend(std::iter::repeat_n(' ', spaces));
                    column += spaces;
                } else {
                    out.push(ch);
                    column += 1;
                }
            }
            out
        })
        .collect();
    replace_block(text, selection, lines, &converted, eol)
}

/// Space to TAB: runs of spaces that reach a tab stop become tabs. `leading_only` limits that
/// to the indentation (Notepad++'s "Space to TAB (Leading)").
pub fn spaces_to_tabs(
    text: &Rope,
    selection: &Selection,
    tab_width: usize,
    leading_only: bool,
    eol: LineEnding,
) -> Option<Transaction> {
    let tab_width = tab_width.max(1);
    let lines = target_lines(text, selection);
    let converted: Vec<String> = line_texts(text, lines.clone())
        .iter()
        .map(|line| {
            let mut out = String::with_capacity(line.len());
            let mut column = 0;
            // Spaces seen since the last tab stop, not written yet.
            let mut pending = 0;
            let mut in_indent = true;
            for ch in line.chars() {
                let convert = !leading_only || in_indent;
                match ch {
                    ' ' if convert => {
                        pending += 1;
                        column += 1;
                        if column % tab_width == 0 {
                            // A single space reaching a stop stays a space.
                            if pending > 1 {
                                out.push('\t');
                            } else {
                                out.push(' ');
                            }
                            pending = 0;
                        }
                    }
                    '\t' => {
                        pending = 0;
                        out.push('\t');
                        column += tab_width - column % tab_width;
                    }
                    _ => {
                        out.extend(std::iter::repeat_n(' ', pending));
                        pending = 0;
                        if ch != ' ' {
                            in_indent = false;
                        }
                        out.push(ch);
                        column += 1;
                    }
                }
            }
            out.extend(std::iter::repeat_n(' ', pending));
            out
        })
        .collect();
    replace_block(text, selection, lines, &converted, eol)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::run;

    const LF: LineEnding = LineEnding::Lf;

    #[test]
    fn trims() {
        assert_eq!(
            run("  a  \n\tb\t", |t, s| trim(t, s, Trim::Trailing, LF)),
            "|  a\n\tb"
        );
        assert_eq!(
            run("  a  \n\tb\t", |t, s| trim(t, s, Trim::Leading, LF)),
            "|a  \nb\t"
        );
        assert_eq!(
            run("  a  \n[\tb\t]", |t, s| trim(t, s, Trim::Both, LF)),
            "  a  \n[b]"
        );
    }

    #[test]
    fn eol_to_space_joins() {
        assert_eq!(
            run("a\n b \n\nc", |t, s| eol_to_space(t, s, false, LF)),
            "|a  b   c"
        );
        assert_eq!(
            run("a\n b \n\nc", |t, s| eol_to_space(t, s, true, LF)),
            "|a b c"
        );
    }

    #[test]
    fn tabs_and_spaces() {
        assert_eq!(
            run("\tx\ta\tb", |t, s| tabs_to_spaces(t, s, 4, LF)),
            "|    x   a   b"
        );
        assert_eq!(
            run("        x   y    z", |t, s| spaces_to_tabs(
                t, s, 4, false, LF
            )),
            "|\t\tx\ty\t z"
        );
        assert_eq!(
            run("        x       y", |t, s| spaces_to_tabs(
                t, s, 4, true, LF
            )),
            "|\t\tx       y"
        );
    }
}
