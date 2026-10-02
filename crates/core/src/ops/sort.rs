//! Line Operations that reorder or drop whole lines: sorting, reversing, shuffling, removing
//! duplicates and empty lines. They work on the selected lines, or on the whole document.

use std::cmp::Ordering;

use ropey::Rope;

use super::{line_texts, replace_block, target_lines};
use crate::line_ending::LineEnding;
use crate::selection::Selection;
use crate::transaction::Transaction;

/// How Sort Lines compares lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    /// By code point, as Notepad++'s "Lexicographically".
    Lexicographic,
    /// Lexicographically, ignoring case.
    IgnoreCase,
    /// By the number at the start of each line.
    Integer,
    /// By the decimal number at the start of each line, with `,` as the separator.
    DecimalComma,
    /// By the decimal number at the start of each line, with `.` as the separator.
    DecimalDot,
    /// By length in characters.
    Length,
}

/// A line that has no number to sort by.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("line {line} does not start with a number")]
pub struct SortError {
    /// 1-based line number, as the user sees it.
    pub line: usize,
}

/// Sorts the selected lines (all lines without a selection). The sort is stable. Numeric keys
/// ignore leading blanks; blank lines sort before every number; a line without a number is an
/// error, as in Notepad++.
pub fn sort_lines(
    text: &Rope,
    selection: &Selection,
    key: SortKey,
    descending: bool,
    eol: LineEnding,
) -> Result<Option<Transaction>, SortError> {
    let lines = target_lines(text, selection);
    let first = lines.start;
    let mut entries: Vec<(usize, String)> = line_texts(text, lines.clone())
        .into_iter()
        .enumerate()
        .collect();
    let compare: fn(&String, &String) -> Ordering = match key {
        SortKey::Lexicographic => |a, b| a.cmp(b),
        SortKey::IgnoreCase => |a, b| a.to_lowercase().cmp(&b.to_lowercase()),
        SortKey::Length => |a, b| a.chars().count().cmp(&b.chars().count()),
        SortKey::Integer | SortKey::DecimalComma | SortKey::DecimalDot => {
            let separator = match key {
                SortKey::DecimalComma => Some(','),
                SortKey::DecimalDot => Some('.'),
                _ => None,
            };
            let mut numbers = Vec::with_capacity(entries.len());
            for (index, content) in &entries {
                match parse_number(content, separator) {
                    Some(number) => numbers.push(number),
                    None => {
                        return Err(SortError {
                            line: first + index + 1,
                        });
                    }
                }
            }
            // Sort indices by number, then rebuild.
            let mut order: Vec<usize> = (0..entries.len()).collect();
            order.sort_by(|&a, &b| {
                let ordering = numbers[a].total_cmp(&numbers[b]);
                if descending {
                    ordering.reverse()
                } else {
                    ordering
                }
            });
            let sorted: Vec<String> = order.into_iter().map(|i| entries[i].1.clone()).collect();
            return Ok(replace_block(text, selection, lines, &sorted, eol));
        }
    };
    entries.sort_by(|a, b| {
        let ordering = compare(&a.1, &b.1);
        if descending {
            ordering.reverse()
        } else {
            ordering
        }
    });
    let sorted: Vec<String> = entries.into_iter().map(|(_, content)| content).collect();
    Ok(replace_block(text, selection, lines, &sorted, eol))
}

/// The number at the start of a line; blank lines count as negative infinity.
fn parse_number(line: &str, decimal: Option<char>) -> Option<f64> {
    let line = line.trim_start_matches([' ', '\t']);
    if line.trim().is_empty() {
        return Some(f64::NEG_INFINITY);
    }
    let mut number = String::new();
    let mut chars = line.chars().peekable();
    if let Some(&sign) = chars.peek()
        && (sign == '-' || sign == '+')
    {
        number.push(sign);
        chars.next();
    }
    let mut seen_digit = false;
    let mut seen_decimal = false;
    for ch in chars {
        match ch {
            '0'..='9' => {
                seen_digit = true;
                number.push(ch);
            }
            _ if Some(ch) == decimal && !seen_decimal => {
                seen_decimal = true;
                number.push('.');
            }
            _ => break,
        }
    }
    if !seen_digit {
        return None;
    }
    number.trim_end_matches('.').parse().ok()
}

/// Reverse Line Order.
pub fn reverse_lines(text: &Rope, selection: &Selection, eol: LineEnding) -> Option<Transaction> {
    let lines = target_lines(text, selection);
    let mut contents = line_texts(text, lines.clone());
    contents.reverse();
    replace_block(text, selection, lines, &contents, eol)
}

/// Randomize Line Order, with a seed so that tests are repeatable.
pub fn shuffle_lines(
    text: &Rope,
    selection: &Selection,
    seed: u64,
    eol: LineEnding,
) -> Option<Transaction> {
    let lines = target_lines(text, selection);
    let mut contents = line_texts(text, lines.clone());
    // xorshift64*: plenty for shuffling lines.
    let mut state = seed | 1;
    let mut next = || {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        state.wrapping_mul(0x2545_f491_4f6c_dd1d)
    };
    for i in (1..contents.len()).rev() {
        let j = (next() % (i as u64 + 1)) as usize;
        contents.swap(i, j);
    }
    replace_block(text, selection, lines, &contents, eol)
}

/// Remove Duplicate Lines (keeping the first of each), or only consecutive duplicates.
pub fn remove_duplicate_lines(
    text: &Rope,
    selection: &Selection,
    consecutive_only: bool,
    eol: LineEnding,
) -> Option<Transaction> {
    let lines = target_lines(text, selection);
    let contents = line_texts(text, lines.clone());
    let mut seen = std::collections::HashSet::new();
    let mut kept: Vec<String> = Vec::with_capacity(contents.len());
    for content in contents {
        let duplicate = if consecutive_only {
            kept.last() == Some(&content)
        } else {
            !seen.insert(content.clone())
        };
        if !duplicate {
            kept.push(content);
        }
    }
    replace_block(text, selection, lines, &kept, eol)
}

/// Remove Empty Lines; with `blank`, also lines of only spaces and tabs.
pub fn remove_empty_lines(
    text: &Rope,
    selection: &Selection,
    blank: bool,
    eol: LineEnding,
) -> Option<Transaction> {
    let lines = target_lines(text, selection);
    let kept: Vec<String> = line_texts(text, lines.clone())
        .into_iter()
        .filter(|content| {
            if blank {
                !content.trim_matches([' ', '\t']).is_empty()
            } else {
                !content.is_empty()
            }
        })
        .collect();
    if kept.is_empty() {
        // Everything goes: keep one empty line where the block was.
        return replace_block(text, selection, lines, &[String::new()], eol);
    }
    replace_block(text, selection, lines, &kept, eol)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::{parse, run, show};

    const LF: LineEnding = LineEnding::Lf;

    fn sort(marked: &str, key: SortKey, descending: bool) -> String {
        let (text, selection) = parse(marked);
        let transaction = sort_lines(&text, &selection, key, descending, LF).unwrap();
        show(&text, &selection, transaction)
    }

    #[test]
    fn sorts_whole_document_or_selected_lines() {
        assert_eq!(
            sort("b\nA\na\nB", SortKey::Lexicographic, false),
            "|A\nB\na\nb"
        );
        assert_eq!(
            sort("b\nA\na\nB", SortKey::IgnoreCase, false),
            "|A\na\nb\nB"
        );
        assert_eq!(
            sort("b\nA\na\nB", SortKey::Lexicographic, true),
            "|b\na\nB\nA"
        );
        assert_eq!(
            sort("z\n[c\nb\na]\nz", SortKey::Lexicographic, false),
            "z\n[a\nb\nc]\nz"
        );
        assert_eq!(sort("ccc\na\nbb", SortKey::Length, false), "|a\nbb\nccc");
    }

    #[test]
    fn sorts_numbers() {
        assert_eq!(
            sort("10\n9\n-1\n\n100 apples", SortKey::Integer, false),
            "|\n-1\n9\n10\n100 apples"
        );
        assert_eq!(
            sort("1,5\n1,25\n-0,5", SortKey::DecimalComma, false),
            "|-0,5\n1,25\n1,5"
        );
        assert_eq!(sort("1.5\n1.25", SortKey::DecimalDot, true), "|1.5\n1.25");
        let (text, selection) = parse("1\nx\n2");
        assert_eq!(
            sort_lines(&text, &selection, SortKey::Integer, false, LF).unwrap_err(),
            SortError { line: 2 }
        );
    }

    #[test]
    fn reverse_shuffle_and_remove() {
        assert_eq!(run("a\nb\nc", |t, s| reverse_lines(t, s, LF)), "|c\nb\na");
        let shuffled = run("1\n2\n3\n4\n5", |t, s| shuffle_lines(t, s, 7, LF));
        let mut lines: Vec<&str> = shuffled.trim_start_matches('|').lines().collect();
        lines.sort_unstable();
        assert_eq!(lines, ["1", "2", "3", "4", "5"], "a permutation");
        assert_eq!(
            run("a\nb\na\na\nc", |t, s| remove_duplicate_lines(
                t, s, false, LF
            )),
            "|a\nb\nc"
        );
        assert_eq!(
            run("a\nb\na\na\nc", |t, s| remove_duplicate_lines(
                t, s, true, LF
            )),
            "|a\nb\na\nc"
        );
        assert_eq!(
            run("a\n\n  \nb", |t, s| remove_empty_lines(t, s, false, LF)),
            "|a\n  \nb"
        );
        assert_eq!(
            run("a\n\n  \nb", |t, s| remove_empty_lines(t, s, true, LF)),
            "|a\nb"
        );
        assert_eq!(
            run("x", |t, s| reverse_lines(t, s, LF)),
            "|x",
            "nothing to do"
        );
    }
}
