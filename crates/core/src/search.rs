//! Finding text in a rope without copying it.
//!
//! A [`Query`] is compiled into a [`Searcher`], which finds matches forward or backward from a
//! position, optionally wrapping around. Matching works on the rope's chunks; only the few bytes
//! around chunk boundaries are copied. The searcher is an enum of matchers so that Extended
//! (`\n`, `\t`) and regular expression modes (phase 3) slot in without changing callers.

use std::ops::Range;

use memchr::memmem;
use ropey::Rope;

use crate::motion::{CharClass, char_class, next_boundary, word_at};

/// How the pattern is interpreted. Only `Normal` exists for now.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum SearchMode {
    /// The pattern is literal text.
    #[default]
    Normal,
}

/// What to search for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Query {
    pub pattern: String,
    pub match_case: bool,
    /// Only matches with a non-word character (or the text edge) on both sides.
    pub whole_word: bool,
    pub mode: SearchMode,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SearchError {
    #[error("the search text is empty")]
    Empty,
}

/// Which way to search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Backward,
}

/// A compiled query.
#[derive(Debug, Clone)]
pub struct Searcher {
    whole_word: bool,
    matcher: Matcher,
}

#[derive(Debug, Clone)]
enum Matcher {
    /// Case-sensitive literal: byte search with `memmem`.
    Exact { needle: Vec<u8> },
    /// Case-insensitive literal: compared character by character after simple case folding.
    Folded { needle: Vec<char> },
}

impl Searcher {
    pub fn new(query: &Query) -> Result<Self, SearchError> {
        if query.pattern.is_empty() {
            return Err(SearchError::Empty);
        }
        let matcher = match (query.mode, query.match_case) {
            (SearchMode::Normal, true) => Matcher::Exact {
                needle: query.pattern.as_bytes().to_vec(),
            },
            (SearchMode::Normal, false) => Matcher::Folded {
                needle: query.pattern.chars().map(fold).collect(),
            },
        };
        Ok(Self {
            whole_word: query.whole_word,
            matcher,
        })
    }

    /// The first match starting at or after `from` and ending at or before `to`.
    pub fn find_in(&self, text: &Rope, range: Range<usize>) -> Option<Range<usize>> {
        let mut start = range.start;
        loop {
            let found = match &self.matcher {
                Matcher::Exact { needle } => find_exact(text, needle, start..range.end)?,
                Matcher::Folded { needle } => find_folded(text, needle, start..range.end)?,
            };
            if !self.whole_word || is_whole_word(text, &found) {
                return Some(found);
            }
            start = crate::motion::next_boundary(text, found.start);
        }
    }

    /// The last match inside `range`.
    pub fn find_last_in(&self, text: &Rope, range: Range<usize>) -> Option<Range<usize>> {
        // Search forward in windows that grow backwards from the end: matches near the end are
        // found without scanning the whole text.
        let mut window = 64 * 1024;
        loop {
            let start = text.floor_char_boundary(range.end.saturating_sub(window).max(range.start));
            let mut last = None;
            let mut from = start;
            while let Some(found) = self.find_in(text, from..range.end) {
                from = crate::motion::next_boundary(text, found.start);
                last = Some(found);
            }
            if last.is_some() || start <= range.start {
                return last;
            }
            window *= 4;
        }
    }

    /// The next match after the caret: forward from `from`, or backward ending at or before
    /// it. With `wrap`, continues from the other end of the text.
    pub fn find(
        &self,
        text: &Rope,
        from: usize,
        direction: Direction,
        wrap: bool,
    ) -> Option<Range<usize>> {
        let len = text.len();
        match direction {
            Direction::Forward => self
                .find_in(text, from..len)
                .or_else(|| wrap.then(|| self.find_in(text, 0..len)).flatten()),
            Direction::Backward => self
                .find_last_in(text, 0..from)
                .or_else(|| wrap.then(|| self.find_last_in(text, 0..len)).flatten()),
        }
    }

    /// Every match, in order, not overlapping.
    pub fn find_all(&self, text: &Rope) -> Vec<Range<usize>> {
        let mut matches = Vec::new();
        let mut from = 0;
        while let Some(found) = self.find_in(text, from..text.len()) {
            from = if found.is_empty() {
                crate::motion::next_boundary(text, found.end)
            } else {
                found.end
            };
            matches.push(found);
            if from >= text.len() {
                break;
            }
        }
        matches
    }

    /// Every match inside `range`, in order, not overlapping.
    pub fn find_all_in(&self, text: &Rope, range: Range<usize>) -> Vec<Range<usize>> {
        let mut scan = Scan::new(range);
        scan.run(self, text, || false);
        scan.into_matches()
    }

    /// Whether `range` of `text` is a match (to decide what "Replace" replaces).
    pub fn is_match(&self, text: &Rope, range: Range<usize>) -> bool {
        self.find_in(text, range.clone()) == Some(range)
    }

    /// The longest a match can be, in bytes. Case folding may change a character's length
    /// (the Kelvin sign is three bytes, `k` one), so a folded match may be longer than its
    /// pattern.
    fn max_match_len(&self) -> usize {
        match &self.matcher {
            Matcher::Exact { needle } => needle.len(),
            Matcher::Folded { needle } => needle.len() * 4,
        }
    }
}

/// Bytes a [`Scan`] searches between two checks of its caller's budget.
pub const SCAN_STEP: usize = 64 * 1024;

/// A search for every match in a range that can stop and resume, so that a caller with a
/// time budget (smart highlighting on the UI thread) can spread it over several frames.
///
/// The result is the same as searching the range in one go: non-overlapping matches, each
/// found as early as possible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scan {
    range: Range<usize>,
    /// Where the next step starts.
    next: usize,
    step: usize,
    matches: Vec<Range<usize>>,
}

impl Scan {
    /// A scan of `range`, whose ends lie on character boundaries.
    pub fn new(range: Range<usize>) -> Self {
        Self::with_step(range, SCAN_STEP)
    }

    /// A scan that searches `step` bytes between checks of the budget.
    pub fn with_step(range: Range<usize>, step: usize) -> Self {
        Self {
            next: range.start,
            range,
            step: step.max(1),
            matches: Vec::new(),
        }
    }

    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }

    pub fn is_done(&self) -> bool {
        self.next >= self.range.end
    }

    /// The matches found so far.
    pub fn matches(&self) -> &[Range<usize>] {
        &self.matches
    }

    pub fn into_matches(self) -> Vec<Range<usize>> {
        self.matches
    }

    /// Searches until the range is done or `stop` returns true; `stop` is asked after every
    /// step, so each call makes progress. Returns whether the range is done.
    pub fn run(
        &mut self,
        searcher: &Searcher,
        text: &Rope,
        mut stop: impl FnMut() -> bool,
    ) -> bool {
        while !self.is_done() {
            self.step(searcher, text);
            if stop() {
                break;
            }
        }
        self.is_done()
    }

    /// Finds the matches that start in the next `step` bytes.
    fn step(&mut self, searcher: &Searcher, text: &Rope) {
        let end = self.range.end;
        let limit = text.ceil_char_boundary((self.next + self.step).min(end));
        // Far enough to see a whole match that starts just before `limit`.
        let window_end =
            text.ceil_char_boundary(limit.saturating_add(searcher.max_match_len()).min(end));
        let mut from = self.next;
        while from < limit {
            match searcher.find_in(text, from..window_end) {
                Some(found) if found.start < limit => {
                    from = if found.is_empty() {
                        next_boundary(text, found.end)
                    } else {
                        found.end
                    };
                    self.matches.push(found);
                }
                _ => break,
            }
        }
        self.next = from.max(limit);
    }
}

/// What smart highlighting looks for when `selection` is selected: the selected text if it
/// is on one line and, with `whole_word`, exactly one word, as in Notepad++.
pub fn smart_highlight_token(
    text: &Rope,
    selection: Range<usize>,
    whole_word: bool,
) -> Option<Range<usize>> {
    if selection.is_empty() {
        return None;
    }
    let mut chars = text.slice(selection.clone()).chars();
    if whole_word {
        let all_word = chars.all(is_word);
        (all_word && is_whole_word(text, &selection)).then_some(selection)
    } else {
        let one_line = !chars.any(|ch| ch == '\n' || ch == '\r');
        one_line.then_some(selection)
    }
}

/// The token of Search > Style All Occurrences of Token: the selection, or the word at the
/// caret when nothing is selected.
pub fn token_at(text: &Rope, selection: Range<usize>) -> Option<Range<usize>> {
    if !selection.is_empty() {
        return Some(selection);
    }
    let word = word_at(text, selection.start);
    let is_word_run = text.slice(word.clone()).chars().next().is_some_and(is_word);
    is_word_run.then_some(word)
}

/// Simple case folding: the first character of the lowercase mapping. Good for the scripts
/// people search in; full folding (ß → ss) comes with regular expressions.
fn fold(ch: char) -> char {
    ch.to_lowercase().next().unwrap_or(ch)
}

fn is_word(ch: char) -> bool {
    char_class(ch) == CharClass::Word
}

fn is_whole_word(text: &Rope, range: &Range<usize>) -> bool {
    let before = text.chars_at(range.start).prev();
    let after = text.chars_at(range.end).next();
    !before.is_some_and(is_word) && !after.is_some_and(is_word)
}

/// Byte search over the chunks of `range`, checking matches that span a chunk boundary.
fn find_exact(text: &Rope, needle: &[u8], range: Range<usize>) -> Option<Range<usize>> {
    if range.len() < needle.len() {
        return None;
    }
    let finder = memmem::Finder::new(needle);
    let slice = text.slice(range.clone());
    let mut chunk_start = range.start;
    // The end of the previous chunk, for matches that straddle two chunks.
    let mut tail: Vec<u8> = Vec::new();
    for chunk in slice.chunks() {
        let bytes = chunk.as_bytes();
        if !tail.is_empty() {
            let take = bytes.len().min(needle.len() - 1);
            let mut seam = tail.clone();
            seam.extend_from_slice(&bytes[..take]);
            if let Some(at) = finder.find(&seam) {
                let start = chunk_start - tail.len() + at;
                return Some(start..start + needle.len());
            }
        }
        if let Some(at) = finder.find(bytes) {
            let start = chunk_start + at;
            return Some(start..start + needle.len());
        }
        let keep = bytes.len().min(needle.len() - 1);
        if keep == bytes.len() {
            tail.extend_from_slice(bytes);
            let excess = tail.len().saturating_sub(needle.len() - 1);
            tail.drain(..excess);
        } else {
            tail.clear();
            tail.extend_from_slice(&bytes[bytes.len() - keep..]);
        }
        chunk_start += bytes.len();
    }
    None
}

/// Case-insensitive search: candidates are positions whose character folds to the needle's
/// first character; each candidate is compared character by character.
fn find_folded(text: &Rope, needle: &[char], range: Range<usize>) -> Option<Range<usize>> {
    let first = needle[0];
    let mut pos = range.start;
    let mut chars = text.slice(range.clone()).chars();
    while let Some(ch) = chars.next() {
        if fold(ch) == first {
            let mut end = pos + ch.len_utf8();
            let mut rest = chars.clone();
            let mut matched = true;
            for &expected in &needle[1..] {
                match rest.next() {
                    Some(next) if fold(next) == expected => end += next.len_utf8(),
                    _ => {
                        matched = false;
                        break;
                    }
                }
            }
            if matched && end <= range.end {
                return Some(pos..end);
            }
        }
        pos += ch.len_utf8();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn searcher(pattern: &str, match_case: bool, whole_word: bool) -> Searcher {
        Searcher::new(&Query {
            pattern: pattern.into(),
            match_case,
            whole_word,
            mode: SearchMode::Normal,
        })
        .unwrap()
    }

    #[test]
    fn finds_forward_and_backward_with_wrap() {
        let text = Rope::from_str("one two one three one");
        let s = searcher("one", true, false);
        assert_eq!(s.find(&text, 0, Direction::Forward, false), Some(0..3));
        assert_eq!(s.find(&text, 1, Direction::Forward, false), Some(8..11));
        assert_eq!(s.find(&text, 19, Direction::Forward, false), None);
        assert_eq!(s.find(&text, 19, Direction::Forward, true), Some(0..3));
        assert_eq!(s.find(&text, 18, Direction::Backward, false), Some(8..11));
        assert_eq!(s.find(&text, 2, Direction::Backward, false), None);
        assert_eq!(s.find(&text, 2, Direction::Backward, true), Some(18..21));
        assert_eq!(s.find_all(&text), [0..3, 8..11, 18..21]);
    }

    #[test]
    fn case_and_whole_word() {
        let text = Rope::from_str("Word word swords WORD Привет привет");
        assert_eq!(
            searcher("word", true, false).find_all(&text),
            [5..9, 11..15]
        );
        assert_eq!(
            searcher("word", false, false).find_all(&text),
            [0..4, 5..9, 11..15, 17..21]
        );
        assert_eq!(
            searcher("word", false, true).find_all(&text),
            [0..4, 5..9, 17..21]
        );
        assert_eq!(searcher("ПРИВЕТ", false, true).find_all(&text).len(), 2);
    }

    #[test]
    fn matches_across_chunk_boundaries() {
        // Ropey chunks are around a kilobyte; put needles at every offset around boundaries.
        let mut source = String::new();
        for i in 0..2000 {
            source.push_str(if i % 97 == 0 { "needle" } else { "hay." });
        }
        let text = Rope::from_str(&source);
        let expected: Vec<Range<usize>> = source
            .match_indices("needle")
            .map(|(at, m)| at..at + m.len())
            .collect();
        assert_eq!(searcher("needle", true, false).find_all(&text), expected);
        assert_eq!(searcher("NEEDLE", false, false).find_all(&text), expected);
        let last = expected.last().unwrap().clone();
        assert_eq!(
            searcher("needle", true, false).find(&text, text.len(), Direction::Backward, false),
            Some(last)
        );
    }

    #[test]
    fn empty_pattern_is_an_error() {
        assert_eq!(
            Searcher::new(&Query::default()).unwrap_err(),
            SearchError::Empty
        );
    }

    #[test]
    fn is_match_checks_the_exact_range() {
        let text = Rope::from_str("abc abc");
        let s = searcher("abc", true, false);
        assert!(s.is_match(&text, 4..7));
        assert!(!s.is_match(&text, 3..7));
    }

    #[test]
    fn a_scan_in_small_steps_finds_what_one_search_finds() {
        let text = Rope::from_str(&"one two one, ONE.  one".repeat(50));
        let s = searcher("one", false, true);
        let whole = s.find_all_in(&text, 3..text.len() - 2);
        assert_eq!(whole.first(), Some(&(8..11)));
        for step in [1, 2, 5, 64] {
            let mut scan = Scan::with_step(3..text.len() - 2, step);
            let mut steps = 0;
            while !scan.run(&s, &text, || true) {
                steps += 1;
            }
            assert!(steps > 0 || step == 64);
            assert_eq!(scan.matches(), whole.as_slice(), "step {step}");
        }
    }

    #[test]
    fn smart_highlighting_wants_a_word_or_one_line() {
        let text = Rope::from_str("foo bar_1 baz\nqux");
        assert_eq!(smart_highlight_token(&text, 4..9, true), Some(4..9));
        assert_eq!(
            smart_highlight_token(&text, 4..7, true),
            None,
            "part of a word"
        );
        assert_eq!(smart_highlight_token(&text, 0..7, true), None, "two words");
        assert_eq!(smart_highlight_token(&text, 4..7, false), Some(4..7));
        assert_eq!(smart_highlight_token(&text, 0..7, false), Some(0..7));
        assert_eq!(
            smart_highlight_token(&text, 10..15, false),
            None,
            "two lines"
        );
        assert_eq!(smart_highlight_token(&text, 4..4, false), None);
    }

    #[test]
    fn the_token_is_the_selection_or_the_word_at_the_caret() {
        let text = Rope::from_str("foo bar, baz");
        assert_eq!(token_at(&text, 1..6), Some(1..6));
        assert_eq!(token_at(&text, 5..5), Some(4..7));
        assert_eq!(token_at(&text, 7..7), Some(4..7), "right after a word");
        assert_eq!(
            token_at(&text, 8..8),
            None,
            "between punctuation and a space"
        );
    }
}
