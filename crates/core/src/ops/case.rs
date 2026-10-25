//! Edit > Convert Case to: applies to every selection; carets alone change nothing.

use ropey::Rope;

use crate::change::Edit;
use crate::selection::{Range as Span, Selection};
use crate::transaction::Transaction;

/// Notepad++'s case conversions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    Upper,
    Lower,
    /// The first letter of every word upper case, the rest lower case.
    Proper,
    /// The first letter of every word upper case, the rest as it was.
    ProperBlend,
    /// The first letter of every sentence upper case, the rest lower case.
    Sentence,
    /// The first letter of every sentence upper case, the rest as it was.
    SentenceBlend,
    Invert,
    /// Each letter upper or lower case at random (seeded, for repeatable tests).
    Random(u64),
}

/// Converts the text of every non-empty selection, keeping the selections on the result.
pub fn convert_case(text: &Rope, selection: &Selection, case: Case) -> Option<Transaction> {
    let mut seed = match case {
        Case::Random(seed) => seed | 1,
        _ => 1,
    };
    replace_selections(text, selection, |original| {
        convert(original, case, &mut seed)
    })
}

/// What Redact Selection puts in place of each character.
pub const REDACTED: char = '\u{2588}';

/// Edit > On Selection > Redact Selection: every character of the selections but line breaks
/// becomes a full block (█), so that the text can be shown without what it says.
pub fn redact(text: &Rope, selection: &Selection) -> Option<Transaction> {
    replace_selections(text, selection, |original| {
        original
            .chars()
            .map(|ch| {
                if matches!(ch, '\r' | '\n') {
                    ch
                } else {
                    REDACTED
                }
            })
            .collect()
    })
}

/// Replaces the text of every non-empty selection with what `convert` makes of it, keeping
/// the selections on the result.
fn replace_selections(
    text: &Rope,
    selection: &Selection,
    mut convert: impl FnMut(&str) -> String,
) -> Option<Transaction> {
    let mut edits = Vec::new();
    let mut ranges = Vec::new();
    let mut shift: isize = 0;
    for range in selection.iter() {
        let start = (range.from() as isize + shift) as usize;
        if range.is_empty() {
            ranges.push(Span::point(start));
            continue;
        }
        let original = text.slice(range.from()..range.to()).to_string();
        let converted = convert(&original);
        let end = start + converted.len();
        ranges.push(if range.is_backward() {
            Span::new(end, start)
        } else {
            Span::new(start, end)
        });
        shift += converted.len() as isize - original.len() as isize;
        if converted != original {
            edits.push(Edit::replace(range.from()..range.to(), converted));
        }
    }
    if edits.is_empty() {
        return None;
    }
    let transaction = Transaction::from_edits(text, edits).ok()?;
    Some(transaction.with_selection(Selection::new(ranges, selection.primary_index())))
}

fn convert(text: &str, case: Case, seed: &mut u64) -> String {
    match case {
        Case::Upper => text.to_uppercase(),
        Case::Lower => text.to_lowercase(),
        Case::Invert => text
            .chars()
            .flat_map(|ch| -> Box<dyn Iterator<Item = char>> {
                if ch.is_uppercase() {
                    Box::new(ch.to_lowercase())
                } else if ch.is_lowercase() {
                    Box::new(ch.to_uppercase())
                } else {
                    Box::new(std::iter::once(ch))
                }
            })
            .collect(),
        Case::Random(_) => text
            .chars()
            .flat_map(|ch| -> Box<dyn Iterator<Item = char>> {
                *seed ^= *seed << 13;
                *seed ^= *seed >> 7;
                *seed ^= *seed << 17;
                if *seed & 1 == 0 {
                    Box::new(ch.to_uppercase())
                } else {
                    Box::new(ch.to_lowercase())
                }
            })
            .collect(),
        Case::Proper | Case::ProperBlend => {
            capitalize(text, case == Case::ProperBlend, |previous| {
                !previous.is_some_and(char::is_alphanumeric)
            })
        }
        Case::Sentence | Case::SentenceBlend => {
            let mut sentence_start = true;
            let mut out = String::with_capacity(text.len());
            for ch in text.chars() {
                if ch.is_alphabetic() && sentence_start {
                    out.extend(ch.to_uppercase());
                    sentence_start = false;
                } else if case == Case::Sentence {
                    out.extend(ch.to_lowercase());
                } else {
                    out.push(ch);
                }
                if matches!(ch, '.' | '!' | '?') {
                    sentence_start = true;
                } else if ch.is_alphanumeric() {
                    sentence_start = false;
                }
            }
            out
        }
    }
}

/// Upper-cases letters where `starts_word(previous char)` says a word begins.
fn capitalize(text: &str, blend: bool, starts_word: impl Fn(Option<char>) -> bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut previous = None;
    for ch in text.chars() {
        if ch.is_alphabetic() && starts_word(previous) {
            out.extend(ch.to_uppercase());
        } else if blend {
            out.push(ch);
        } else {
            out.extend(ch.to_lowercase());
        }
        previous = Some(ch);
    }
    out
}
