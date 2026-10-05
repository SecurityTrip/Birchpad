//! Multi-select: adding the next occurrence (or every occurrence) of the selected text, or of
//! the word at the caret, to the selection, as Notepad++'s Multi-select Next and Multi-select
//! All (Scintilla's `SCI_MULTIPLESELECTADDNEXT` and `SCI_MULTIPLESELECTADDEACH`).

use std::ops::Range as ByteRange;

use ropey::Rope;

use crate::motion::{CharClass, char_class, word_at};
use crate::search::{Query, Searcher};
use crate::selection::{Range, Selection};

/// How occurrences are matched.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MatchOptions {
    pub match_case: bool,
    pub whole_word: bool,
}

/// The word at `pos`, if there is one (not spaces or punctuation).
fn word_range(text: &Rope, pos: usize) -> Option<ByteRange<usize>> {
    let range = word_at(text, pos);
    let first = text.slice(range.clone()).chars().next()?;
    (char_class(first) == CharClass::Word).then_some(range)
}

fn searcher(text: &Rope, range: ByteRange<usize>, options: MatchOptions) -> Option<Searcher> {
    Searcher::new(&Query {
        pattern: text.slice(range).to_string(),
        match_case: options.match_case,
        whole_word: options.whole_word,
        ..Query::default()
    })
    .ok()
}

fn overlaps_selection(selection: &Selection, found: &ByteRange<usize>) -> bool {
    selection.iter().any(|range| {
        (range.from() < found.end && found.start < range.to()) || range.from() == found.start
    })
}

/// Multi-select Next: with a caret, selects the word at it; with a selection, adds the next
/// occurrence of the primary range's text after it (wrapping around) and makes it primary.
/// `None` when there is nothing (more) to select.
pub fn select_next(text: &Rope, selection: &Selection, options: MatchOptions) -> Option<Selection> {
    let primary = selection.primary();
    if primary.is_empty() {
        let word = word_range(text, primary.head)?;
        let index = selection.primary_index();
        let mut ranges = selection.ranges().to_vec();
        ranges[index] = Range::new(word.start, word.end);
        return Some(Selection::new(ranges, index));
    }
    let searcher = searcher(text, primary.from()..primary.to(), options)?;
    next_after(text, selection, &searcher, primary.to())
        .map(|found| selection.clone().push(Range::new(found.start, found.end)))
}

/// The first match at or after `pos` (wrapping around) that is not selected yet.
fn next_after(
    text: &Rope,
    selection: &Selection,
    searcher: &Searcher,
    pos: usize,
) -> Option<ByteRange<usize>> {
    let fresh = |range: &ByteRange<usize>| !overlaps_selection(selection, range);
    let mut from = pos;
    while let Some(found) = searcher.find_in(text, from..text.len()) {
        if fresh(&found) {
            return Some(found);
        }
        from = crate::motion::next_boundary(text, found.start);
    }
    let mut from = 0;
    while let Some(found) = searcher.find_in(text, from..pos.min(text.len())) {
        if fresh(&found) {
            return Some(found);
        }
        from = crate::motion::next_boundary(text, found.start);
    }
    None
}

/// Skip Current & Go to Next Multi-select: drops the primary range and selects the next
/// occurrence of its text instead.
pub fn skip_to_next(
    text: &Rope,
    selection: &Selection,
    options: MatchOptions,
) -> Option<Selection> {
    let primary = selection.primary();
    if primary.is_empty() {
        return select_next(text, selection, options);
    }
    let searcher = searcher(text, primary.from()..primary.to(), options)?;
    let found = next_after(text, selection, &searcher, primary.to())?;
    let found = Range::new(found.start, found.end);
    Some(match selection.remove(selection.primary_index()) {
        Some(rest) => rest.push(found),
        None => Selection::single(found),
    })
}

/// Multi-select All: adds every occurrence of the primary range's text (or of the word at the
/// caret) to the selection. The primary range stays primary.
pub fn select_all(text: &Rope, selection: &Selection, options: MatchOptions) -> Option<Selection> {
    let primary = selection.primary();
    let target = if primary.is_empty() {
        word_range(text, primary.head)?
    } else {
        primary.from()..primary.to()
    };
    let searcher = searcher(text, target.clone(), options)?;
    let matches = searcher.find_all(text);
    if matches.is_empty() {
        return None;
    }
    let mut ranges: Vec<Range> = selection
        .iter()
        .filter(|range| !range.is_empty())
        .copied()
        .collect();
    let primary_range = Range::new(target.start, target.end);
    ranges.push(primary_range);
    let primary_index = ranges.len() - 1;
    ranges.extend(
        matches
            .into_iter()
            .map(|found| Range::new(found.start, found.end)),
    );
    Some(Selection::new(ranges, primary_index))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::{parse, show};

    fn run(
        marked: &str,
        options: MatchOptions,
        op: fn(&Rope, &Selection, MatchOptions) -> Option<Selection>,
    ) -> String {
        let (text, selection) = parse(marked);
        match op(&text, &selection, options) {
            Some(selection) => show(&text, &selection, None),
            None => "none".to_owned(),
        }
    }

    const ANY: MatchOptions = MatchOptions {
        match_case: false,
        whole_word: false,
    };

    #[test]
    fn next_selects_the_word_then_adds_occurrences() {
        assert_eq!(
            run("fo|o bar Foo foo", ANY, select_next),
            "[foo] bar Foo foo"
        );
        assert_eq!(
            run("[foo] bar Foo foo", ANY, select_next),
            "[foo] bar [Foo] foo"
        );
        let case = MatchOptions {
            match_case: true,
            ..ANY
        };
        assert_eq!(
            run("[foo] bar Foo foo", case, select_next),
            "[foo] bar Foo [foo]"
        );
        assert_eq!(
            run("a | b", ANY, select_next),
            "none",
            "no word at the caret"
        );
    }

    #[test]
    fn next_wraps_and_skips_selected_occurrences() {
        let (text, selection) = parse("ab [ab] ab");
        let second = select_next(&text, &selection, ANY).unwrap();
        assert_eq!(show(&text, &second, None), "ab [ab] [ab]");
        assert_eq!(second.primary(), Range::new(6, 8));
        let third = select_next(&text, &second, ANY).unwrap();
        assert_eq!(
            show(&text, &third, None),
            "[ab] [ab] [ab]",
            "wrapped around"
        );
        assert_eq!(third.primary(), Range::new(0, 2));
        assert!(select_next(&text, &third, ANY).is_none(), "all selected");
    }

    #[test]
    fn whole_word_skips_parts_of_words() {
        let whole = MatchOptions {
            whole_word: true,
            ..ANY
        };
        assert_eq!(
            run("[cat] category cat", whole, select_next),
            "[cat] category [cat]"
        );
        assert_eq!(
            run("c|at category cat", whole, select_all),
            "[cat] category [cat]"
        );
        assert_eq!(
            run("c|at category cat", ANY, select_all),
            "[cat] [cat]egory [cat]"
        );
    }

    #[test]
    fn select_all_keeps_the_primary_and_skip_moves_on() {
        let (text, selection) = parse("x y |x x");
        let all = select_all(&text, &selection, ANY).unwrap();
        assert_eq!(show(&text, &all, None), "[x] y [x] [x]");
        assert_eq!(all.primary(), Range::new(4, 5));
        assert_eq!(run("[x] y x x", ANY, skip_to_next), "x y [x] x");
        let (text, selection) = parse("[x] y [x] x");
        let skipped = skip_to_next(&text, &selection.with_primary_index(1), ANY).unwrap();
        assert_eq!(show(&text, &skipped, None), "[x] y x [x]");
        assert_eq!(run("[only] one", ANY, skip_to_next), "none");
    }
}
