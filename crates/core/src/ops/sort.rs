//! Line Operations that reorder or drop whole lines: sorting, reversing, shuffling, removing
//! duplicates and empty lines. They work on the selected lines, or on the whole document.

use std::cmp::Ordering;

use ropey::Rope;

use super::{line_texts, replace_block, target_lines};
use crate::line_ending::LineEnding;
use crate::motion::{line_of, line_range};
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
    /// Sort in Locale Order: letters first without regard to accents and case, then accents,
    /// then lower case before capitals (Unicode's default order in short, not each language's
    /// own rules).
    Locale,
}

/// A line that has no number to sort by.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{}", birchpad_i18n::t!("core.sort.not-a-number", line = .line))]
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
    let contents = line_texts(text, lines.clone());
    let keys = contents.clone();
    sort_with_keys(text, selection, lines, contents, keys, key, descending, eol)
}

/// Sorts whole lines by the text of a rectangular selection's columns, as Notepad++ does when
/// a column selection is active: `block` has one range per line (lines it skips, such as
/// hidden ones, sort by an empty key). With a zero-width rectangle, each key runs from the
/// caret to the end of its line.
pub fn sort_lines_by_columns(
    text: &Rope,
    block: &Selection,
    key: SortKey,
    descending: bool,
    eol: LineEnding,
) -> Result<Option<Transaction>, SortError> {
    let (Some(first), Some(last)) = (block.ranges().first(), block.ranges().last()) else {
        return Ok(None);
    };
    let lines = line_of(text, first.from())..line_of(text, last.to()) + 1;
    let thin = block.iter().all(|range| range.from() == range.to());
    let mut keys = vec![String::new(); lines.len()];
    for range in block.iter() {
        let line = line_of(text, range.from());
        let end = if thin {
            line_range(text, line).end
        } else {
            range.to()
        };
        keys[line - lines.start] = text.slice(range.from()..end).to_string();
    }
    let contents = line_texts(text, lines.clone());
    sort_with_keys(text, block, lines, contents, keys, key, descending, eol)
}

/// Whether `ch` is a combining mark that an accented letter decomposes into.
fn is_mark(ch: char) -> bool {
    matches!(
        ch,
        '\u{0300}'..='\u{036F}'
            | '\u{1AB0}'..='\u{1AFF}'
            | '\u{1DC0}'..='\u{1DFF}'
            | '\u{20D0}'..='\u{20FF}'
            | '\u{FE20}'..='\u{FE2F}'
    )
}

/// What Sort in Locale Order compares, in turn: the letters in lower case without accents,
/// then with them, then the case of each letter (lower case first). Cyrillic й, a letter of its
/// own though it decomposes, stays one, after и.
pub fn collation_key(text: &str) -> (String, String, String) {
    let decomposed = icu_normalizer::DecomposingNormalizerBorrowed::new_nfd().normalize(text);
    let mut primary = String::with_capacity(text.len());
    let mut secondary = String::with_capacity(text.len());
    let mut previous: Option<char> = None;
    for ch in decomposed.chars() {
        let lower = ch.to_lowercase().next().unwrap_or(ch);
        secondary.push(lower);
        if ch == '\u{0306}' && matches!(previous, Some('и' | 'И')) {
            primary.pop();
            primary.push('й');
        } else if !is_mark(ch) {
            primary.push(lower);
        }
        previous = Some(ch);
    }
    let tertiary = text
        .chars()
        .map(|ch| if ch.is_uppercase() { '1' } else { '0' })
        .collect();
    (primary, secondary, tertiary)
}

/// Reorders `contents` (the lines `lines`) by `keys`, stably.
#[expect(
    clippy::too_many_arguments,
    reason = "the two public entry points differ only in how they get the keys"
)]
fn sort_with_keys(
    text: &Rope,
    selection: &Selection,
    lines: std::ops::Range<usize>,
    contents: Vec<String>,
    keys: Vec<String>,
    key: SortKey,
    descending: bool,
    eol: LineEnding,
) -> Result<Option<Transaction>, SortError> {
    let first = lines.start;
    let mut order: Vec<usize> = (0..contents.len()).collect();
    let direction = |ordering: Ordering| {
        if descending {
            ordering.reverse()
        } else {
            ordering
        }
    };
    match key {
        SortKey::Lexicographic => order.sort_by(|&a, &b| direction(keys[a].cmp(&keys[b]))),
        SortKey::IgnoreCase => {
            let folded: Vec<String> = keys.iter().map(|key| key.to_lowercase()).collect();
            order.sort_by(|&a, &b| direction(folded[a].cmp(&folded[b])));
        }
        SortKey::Locale => {
            let collated: Vec<(String, String, String)> =
                keys.iter().map(|key| collation_key(key)).collect();
            order.sort_by(|&a, &b| {
                direction(
                    collated[a]
                        .cmp(&collated[b])
                        .then_with(|| keys[a].cmp(&keys[b])),
                )
            });
        }
        SortKey::Length => {
            let lengths: Vec<usize> = keys.iter().map(|key| key.chars().count()).collect();
            order.sort_by(|&a, &b| direction(lengths[a].cmp(&lengths[b])));
        }
        SortKey::Integer | SortKey::DecimalComma | SortKey::DecimalDot => {
            let separator = match key {
                SortKey::DecimalComma => Some(','),
                SortKey::DecimalDot => Some('.'),
                _ => None,
            };
            let mut numbers = Vec::with_capacity(keys.len());
            for (index, content) in keys.iter().enumerate() {
                match parse_number(content, separator) {
                    Some(number) => numbers.push(number),
                    None => {
                        return Err(SortError {
                            line: first + index + 1,
                        });
                    }
                }
            }
            order.sort_by(|&a, &b| direction(numbers[a].total_cmp(&numbers[b])));
        }
    }
    let sorted: Vec<String> = order.into_iter().map(|i| contents[i].clone()).collect();
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
