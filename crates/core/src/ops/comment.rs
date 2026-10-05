//! Edit > Comment/Uncomment: line comments (Ctrl+Q, Ctrl+K, Ctrl+Shift+K) and block comments
//! (Ctrl+Shift+Q), with the tokens of the document's language.
//!
//! Line comments go at the smallest indentation of the lines, so a commented block stays
//! aligned; a space follows the token. Uncommenting removes the token and one space after it.
//! A language without line comments (CSS, HTML, XML) gets each line wrapped in a block comment.

use ropey::Rope;

use super::line_blocks;
use crate::change::Edit;
use crate::motion::{indent_end, line_range};
use crate::selection::Selection;
use crate::transaction::Transaction;

/// The comment tokens of a language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommentTokens<'a> {
    pub line: Option<&'a str>,
    pub block: Option<(&'a str, &'a str)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Add,
    Remove,
    Toggle,
}

/// Ctrl+Q: comments the lines of each selection, or uncomments them if every non-blank one is
/// commented.
pub fn toggle_comment(
    text: &Rope,
    selection: &Selection,
    tokens: CommentTokens,
) -> Option<Transaction> {
    line_comments(text, selection, tokens, Mode::Toggle)
}

/// Ctrl+K: comments the lines of each selection.
pub fn comment_lines(
    text: &Rope,
    selection: &Selection,
    tokens: CommentTokens,
) -> Option<Transaction> {
    line_comments(text, selection, tokens, Mode::Add)
}

/// Ctrl+Shift+K: uncomments the commented lines of each selection.
pub fn uncomment_lines(
    text: &Rope,
    selection: &Selection,
    tokens: CommentTokens,
) -> Option<Transaction> {
    line_comments(text, selection, tokens, Mode::Remove)
}

fn line_comments(
    text: &Rope,
    selection: &Selection,
    tokens: CommentTokens,
    mode: Mode,
) -> Option<Transaction> {
    // A line token, or a block pair used on each line.
    let (open, close) = match (tokens.line, tokens.block) {
        (Some(line), _) => (line, None),
        (None, Some((open, close))) => (open, Some(close)),
        (None, None) => return None,
    };
    let mut edits = Vec::new();
    for block in line_blocks(text, selection) {
        let lines: Vec<usize> = block
            .filter(|&line| indent_end(text, line) < line_range(text, line).end)
            .collect();
        if lines.is_empty() {
            continue;
        }
        let commented = |line: usize| {
            let content = text.slice(indent_end(text, line)..line_range(text, line).end);
            let content = content.to_string();
            content.starts_with(open) && close.is_none_or(|close| content.ends_with(close))
        };
        let remove = match mode {
            Mode::Add => false,
            Mode::Remove => true,
            Mode::Toggle => lines.iter().all(|&line| commented(line)),
        };
        let indent = lines
            .iter()
            .map(|&line| indent_end(text, line) - line_range(text, line).start)
            .min()
            .unwrap_or(0);
        for &line in &lines {
            let range = line_range(text, line);
            if remove {
                if !commented(line) {
                    continue;
                }
                let start = indent_end(text, line);
                let mut end = start + open.len();
                if text.get_byte(end) == Some(b' ') {
                    end += 1;
                }
                edits.push(Edit::delete(start..end));
                if let Some(close) = close {
                    let mut from = range.end - close.len();
                    if from > end && text.byte(from - 1) == b' ' {
                        from -= 1;
                    }
                    edits.push(Edit::delete(from..range.end));
                }
            } else {
                // Column `indent` is a byte offset of the line: indentation is ASCII.
                edits.push(Edit::insert(range.start + indent, format!("{open} ")));
                if let Some(close) = close {
                    edits.push(Edit::insert(range.end, format!(" {close}")));
                }
            }
        }
    }
    if edits.is_empty() {
        return None;
    }
    edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
    Transaction::from_edits(text, edits).ok()
}

/// Ctrl+Shift+Q: wraps each selection in a block comment, or unwraps it if it is one. A caret
/// alone takes its line. Without block comments in the language, comments the lines instead.
pub fn block_comment(
    text: &Rope,
    selection: &Selection,
    tokens: CommentTokens,
) -> Option<Transaction> {
    let Some((open, close)) = tokens.block else {
        return comment_lines(text, selection, tokens);
    };
    let mut edits = Vec::new();
    for range in selection.iter() {
        let (start, end) = if range.is_empty() {
            let line = crate::motion::line_of(text, range.head);
            (indent_end(text, line), line_range(text, line).end)
        } else {
            (range.from(), range.to())
        };
        if start >= end {
            continue;
        }
        let content = text.slice(start..end).to_string();
        let trimmed = content.trim();
        if trimmed.starts_with(open)
            && trimmed.ends_with(close)
            && trimmed.len() >= open.len() + close.len()
        {
            let lead = content.len() - content.trim_start().len();
            let inner = &trimmed[open.len()..trimmed.len() - close.len()];
            let inner = inner.strip_prefix(' ').unwrap_or(inner);
            let inner = inner.strip_suffix(' ').unwrap_or(inner);
            let trail = content.len() - content.trim_end().len();
            let replacement = format!(
                "{}{inner}{}",
                &content[..lead],
                &content[content.len() - trail..]
            );
            edits.push(Edit::replace(start..end, replacement));
        } else {
            edits.push(Edit::replace(
                start..end,
                format!("{open} {content} {close}"),
            ));
        }
    }
    if edits.is_empty() {
        return None;
    }
    Transaction::from_edits(text, edits).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::run;

    const RUST: CommentTokens = CommentTokens {
        line: Some("//"),
        block: Some(("/*", "*/")),
    };
    const CSS: CommentTokens = CommentTokens {
        line: None,
        block: Some(("/*", "*/")),
    };

    #[test]
    fn toggles_line_comments_aligned_to_the_block() {
        let toggled = run("[  a\n    b\n\n  c]", |t, s| toggle_comment(t, s, RUST));
        assert_eq!(toggled, "[  // a\n  //   b\n\n  // c]");
        let back = run(&toggled, |t, s| toggle_comment(t, s, RUST));
        assert_eq!(back, "[  a\n    b\n\n  c]");
        // A partly commented block gets commented.
        assert_eq!(
            run("[// a\nb]", |t, s| toggle_comment(t, s, RUST)),
            "[// // a\n// b]"
        );
        assert_eq!(run("x|y", |t, s| toggle_comment(t, s, RUST)), "// x|y");
    }

    #[test]
    fn add_and_remove_line_comments() {
        assert_eq!(
            run("[a\n// b]", |t, s| comment_lines(t, s, RUST)),
            "[// a\n// // b]"
        );
        assert_eq!(
            run("[//a\n// b\nc]", |t, s| uncomment_lines(t, s, RUST)),
            "[a\nb\nc]"
        );
        // The selection end stays before the text added at it.
        assert_eq!(
            run("[a\nb]", |t, s| toggle_comment(t, s, CSS)),
            "[/* a */\n/* b] */"
        );
        assert_eq!(run("[/* a */]", |t, s| toggle_comment(t, s, CSS)), "[a]");
    }

    #[test]
    fn block_comments() {
        assert_eq!(
            run("x [y z] w", |t, s| block_comment(t, s, RUST)),
            "x [/* y z */] w"
        );
        assert_eq!(
            run("x [/* y */] w", |t, s| block_comment(t, s, RUST)),
            "x [y] w"
        );
        assert_eq!(
            run("  co|de", |t, s| block_comment(t, s, RUST)),
            "  /* code */|"
        );
    }
}
