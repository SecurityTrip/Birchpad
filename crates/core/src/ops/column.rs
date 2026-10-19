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
#[path = "../../tests/unit/ops/column/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/unit/ops/column/properties.rs"]
mod properties;
