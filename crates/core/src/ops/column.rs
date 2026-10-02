//! Column editing: inserting text or a sequence of numbers into every row of a rectangular
//! selection or block of carets (Notepad++'s Column Editor), and pasting a rectangle.
//!
//! Which byte range of each line a column covers depends on tab stops and character widths,
//! so the caller (the view) works out the targets; this module only builds the edit.

use ropey::Rope;

use crate::change::Edit;
use crate::line_ending::LineEnding;
use crate::selection::{Range, Selection};
use crate::transaction::Transaction;

/// The base numbers are written in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Base {
    #[default]
    Dec,
    Hex,
    Oct,
    Bin,
}

impl Base {
    pub fn radix(self) -> u32 {
        match self {
            Self::Dec => 10,
            Self::Hex => 16,
            Self::Oct => 8,
            Self::Bin => 2,
        }
    }
}

/// What fills numbers up to the width of the widest one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Leading {
    /// Numbers are left-aligned; spaces after them keep the following text aligned.
    #[default]
    None,
    /// Zeros before the digits (after a minus sign).
    Zeros,
    /// Spaces before the number: right-aligned.
    Spaces,
}

/// Number to Insert of the Column Editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NumberSequence {
    pub initial: i64,
    /// Added after every `repeat` rows; 0 repeats the initial number.
    pub step: i64,
    /// How many rows get the same number; 0 counts as 1.
    pub repeat: usize,
    pub leading: Leading,
    pub base: Base,
    /// Hexadecimal digits A-F rather than a-f.
    pub uppercase: bool,
}

impl Default for NumberSequence {
    fn default() -> Self {
        Self {
            initial: 1,
            step: 1,
            repeat: 1,
            leading: Leading::None,
            base: Base::Dec,
            uppercase: true,
        }
    }
}

impl NumberSequence {
    /// The numbers for `rows` rows, all as wide as the widest, so that what follows them
    /// stays aligned.
    pub fn generate(&self, rows: usize) -> Vec<String> {
        let repeat = self.repeat.max(1) as i64;
        let numbers: Vec<String> = (0..rows as i64)
            .map(|row| {
                let value = self
                    .initial
                    .saturating_add(self.step.saturating_mul(row / repeat));
                format_number(value, self.base, self.uppercase)
            })
            .collect();
        let width = numbers.iter().map(String::len).max().unwrap_or(0);
        numbers
            .into_iter()
            .map(|number| {
                let pad = width - number.len();
                match self.leading {
                    Leading::None => format!("{number}{}", " ".repeat(pad)),
                    Leading::Spaces => format!("{}{number}", " ".repeat(pad)),
                    Leading::Zeros => match number.strip_prefix('-') {
                        Some(digits) => format!("-{}{digits}", "0".repeat(pad)),
                        None => format!("{}{number}", "0".repeat(pad)),
                    },
                }
            })
            .collect()
    }
}

/// `value` in `base`, with a minus sign for negative numbers.
pub fn format_number(value: i64, base: Base, uppercase: bool) -> String {
    let magnitude = value.unsigned_abs();
    let digits = match base {
        Base::Dec => magnitude.to_string(),
        Base::Hex if uppercase => format!("{magnitude:X}"),
        Base::Hex => format!("{magnitude:x}"),
        Base::Oct => format!("{magnitude:o}"),
        Base::Bin => format!("{magnitude:b}"),
    };
    if value < 0 {
        format!("-{digits}")
    } else {
        digits
    }
}

/// Parses a number typed in `base` (the Column Editor's fields use the chosen format). An
/// empty field is `None`, as is anything that is not a number.
pub fn parse_number(text: &str, base: Base) -> Option<i64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    i64::from_str_radix(text, base.radix()).ok()
}

/// Replaces each target range (one per line, top to bottom) with the row of the same index,
/// filling virtual space before it with spaces. Rows past the last target go on new lines
/// added at the end of the text, indented by `column` spaces, as Scintilla pastes a rectangle
/// that does not fit. Targets past the last row are deleted. Leaves a caret after each row.
///
/// # Panics
///
/// Panics if the targets are not sorted, disjoint and on character boundaries.
pub fn insert_rows(
    text: &Rope,
    targets: &[Range],
    rows: &[String],
    column: usize,
    eol: LineEnding,
) -> Transaction {
    let mut edits = Vec::with_capacity(rows.len().max(targets.len()));
    let mut carets = Vec::with_capacity(rows.len());
    let mut shift: isize = 0;
    for (index, target) in targets.iter().enumerate() {
        let row = rows.get(index).map_or("", String::as_str);
        let inserted = if row.is_empty() {
            String::new()
        } else {
            format!("{}{row}", " ".repeat(target.from_virtual()))
        };
        let start = (target.from() as isize + shift) as usize;
        carets.push(if row.is_empty() {
            Range::virtual_point(start, target.from_virtual())
        } else {
            Range::point(start + inserted.len())
        });
        shift += inserted.len() as isize - target.len() as isize;
        edits.push(Edit::replace(target.from()..target.to(), inserted));
    }
    if rows.len() > targets.len() {
        let mut appended = String::new();
        let mut end = (text.len() as isize + shift) as usize;
        for row in &rows[targets.len()..] {
            appended.push_str(eol.as_str());
            appended.push_str(&" ".repeat(column));
            appended.push_str(row);
            end += eol.as_str().len() + column + row.len();
            carets.push(Range::point(end));
        }
        edits.push(Edit::insert(text.len(), appended));
    }
    let primary = carets.len().saturating_sub(1);
    let selection = if carets.is_empty() {
        Selection::point(0)
    } else {
        Selection::new(carets, primary)
    };
    Transaction::from_edits(text, edits)
        .expect("targets are sorted, disjoint and on character boundaries")
        .with_selection(selection)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbers(sequence: NumberSequence, rows: usize) -> Vec<String> {
        sequence.generate(rows)
    }

    #[test]
    fn numbers_are_padded_to_the_widest() {
        let plain = NumberSequence::default();
        assert_eq!(numbers(plain, 3), ["1", "2", "3"]);
        let ten = numbers(plain, 10);
        assert_eq!(ten[0], "1 ", "left-aligned with trailing spaces");
        assert_eq!(ten[9], "10");
        let zeros = NumberSequence {
            leading: Leading::Zeros,
            initial: 8,
            ..plain
        };
        assert_eq!(numbers(zeros, 3), ["08", "09", "10"]);
        let spaces = NumberSequence {
            leading: Leading::Spaces,
            initial: 9,
            ..plain
        };
        assert_eq!(numbers(spaces, 2), [" 9", "10"]);
    }

    #[test]
    fn step_repeat_and_negative_numbers() {
        let sequence = NumberSequence {
            initial: 2,
            step: -3,
            repeat: 2,
            leading: Leading::Zeros,
            ..NumberSequence::default()
        };
        assert_eq!(numbers(sequence, 6), ["02", "02", "-1", "-1", "-4", "-4"]);
        let constant = NumberSequence {
            step: 0,
            initial: 7,
            ..NumberSequence::default()
        };
        assert_eq!(numbers(constant, 2), ["7", "7"]);
    }

    #[test]
    fn other_bases() {
        let hex = NumberSequence {
            initial: 1,
            step: 11,
            base: Base::Hex,
            ..NumberSequence::default()
        };
        assert_eq!(numbers(hex, 5), ["1 ", "C ", "17", "22", "2D"]);
        let lower = NumberSequence {
            uppercase: false,
            ..hex
        };
        assert_eq!(numbers(lower, 5)[4], "2d");
        let binary = NumberSequence {
            base: Base::Bin,
            leading: Leading::Zeros,
            initial: 0,
            ..NumberSequence::default()
        };
        assert_eq!(numbers(binary, 4), ["00", "01", "10", "11"]);
        assert_eq!(format_number(8, Base::Oct, true), "10");
        assert_eq!(parse_number("ff", Base::Hex), Some(255));
        assert_eq!(parse_number(" -12 ", Base::Dec), Some(-12));
        assert_eq!(parse_number("12", Base::Bin), None);
        assert_eq!(parse_number("", Base::Dec), None);
    }

    #[test]
    fn rows_go_into_targets_and_past_the_end() {
        let text = Rope::from_str("abc\nx\nlast");
        // Column 2: inside "abc", virtual space after "x"; then two more lines than exist.
        let targets = [Range::point(2), Range::virtual_point(5, 1), Range::point(8)];
        let rows: Vec<String> = ["1", "2", "3", "4"].map(String::from).to_vec();
        let transaction = insert_rows(&text, &targets, &rows, 2, LineEnding::Lf);
        let mut result = text.clone();
        transaction.changes().apply(&mut result);
        assert_eq!(result, "ab1c\nx 2\nla3st\n  4");
        let carets: Vec<usize> = transaction
            .selection()
            .unwrap()
            .iter()
            .map(|range| range.head)
            .collect();
        assert_eq!(carets, [3, 8, 12, 18]);

        // A wider rectangle is replaced; extra target rows are emptied.
        let targets = [Range::new(0, 2), Range::new(4, 5)];
        let transaction = insert_rows(&text, &targets, &["Z".to_owned()], 0, LineEnding::Lf);
        let mut result = text.clone();
        transaction.changes().apply(&mut result);
        assert_eq!(result, "Zc\n\nlast");
    }
}
