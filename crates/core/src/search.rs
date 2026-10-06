//! Finding text in a rope.
//!
//! A [`Query`] is compiled into a [`Searcher`], which finds matches forward or backward from a
//! position, optionally wrapping around, and says what replaces each. Three modes, as in
//! Notepad++:
//!
//! - **Normal**: the pattern is literal text. Matching works on the rope's chunks; only the few
//!   bytes around chunk boundaries are copied.
//! - **Extended**: literal text with escapes (`\n`, `\t`, `\x41`, ...), see [`unescape_extended`].
//! - **Regular expression**: Perl syntax with back-references and look-around
//!   ([`fancy_regex`]). `^` and `$` match at every line (CRLF included), `.` matches line
//!   breaks only with [`Query::dot_matches_newline`]. Regular expressions run on a copy of the
//!   text, made once per operation, so that look-behind and anchors see the whole text even
//!   when only a part (a selection) is searched.

use std::ops::Range;
use std::sync::{Arc, Mutex};

use fancy_regex::{Regex, RegexBuilder, RegexInput};
use memchr::memmem;
use ropey::Rope;

use crate::motion::{CharClass, char_class, next_boundary, word_at};

/// How the pattern is interpreted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum SearchMode {
    /// The pattern is literal text.
    #[default]
    Normal,
    /// Literal text with escapes: `\n`, `\r`, `\t`, `\0`, `\\`, `\xHH`, `\uHHHH`, `\oNNN`,
    /// `\dNNN`, `\bNNNNNNNN`.
    Extended,
    /// A regular expression.
    Regex,
}

/// What to search for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Query {
    pub pattern: String,
    pub match_case: bool,
    /// Only matches with a non-word character (or the text edge) on both sides. Not used by
    /// regular expressions, which say it with `\b`.
    pub whole_word: bool,
    pub mode: SearchMode,
    /// Regular expressions: `.` also matches `\r` and `\n`.
    pub dot_matches_newline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SearchError {
    #[error("the search text is empty")]
    Empty,
    #[error("invalid regular expression: {0}")]
    InvalidRegex(String),
    /// The regular expression could not be matched, typically because backtracking took too
    /// long (`(a*)*b` on a long run of `a`).
    #[error("the regular expression is too complex to match: {0}")]
    RegexFailed(String),
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
    mode: SearchMode,
    matcher: Matcher,
    /// The error that stopped the last regular expression search, if any: searches report "no
    /// match" and leave the reason here.
    failure: Arc<Mutex<Option<SearchError>>>,
}

#[derive(Debug, Clone)]
enum Matcher {
    /// Case-sensitive literal: byte search with `memmem`.
    Exact {
        needle: Vec<u8>,
    },
    /// Case-insensitive literal: compared character by character after simple case folding.
    Folded {
        needle: Vec<char>,
    },
    Regex(Arc<Regex>),
}

/// How many backtracking steps a regular expression may take for one match before it fails.
const BACKTRACK_LIMIT: usize = 10_000_000;

impl Searcher {
    pub fn new(query: &Query) -> Result<Self, SearchError> {
        if query.pattern.is_empty() {
            return Err(SearchError::Empty);
        }
        let literal = |pattern: &str| {
            if pattern.is_empty() {
                Err(SearchError::Empty)
            } else if query.match_case {
                Ok(Matcher::Exact {
                    needle: pattern.as_bytes().to_vec(),
                })
            } else {
                Ok(Matcher::Folded {
                    needle: pattern.chars().map(fold).collect(),
                })
            }
        };
        let matcher = match query.mode {
            SearchMode::Normal => literal(&query.pattern)?,
            SearchMode::Extended => literal(&unescape_extended(&query.pattern))?,
            SearchMode::Regex => {
                let regex = RegexBuilder::new(&query.pattern)
                    .case_insensitive(!query.match_case)
                    .multi_line(true)
                    .crlf(true)
                    .dot_matches_new_line(query.dot_matches_newline)
                    .backtrack_limit(BACKTRACK_LIMIT)
                    .build()
                    .map_err(|error| SearchError::InvalidRegex(error.to_string()))?;
                Matcher::Regex(Arc::new(regex))
            }
        };
        Ok(Self {
            whole_word: query.whole_word && query.mode != SearchMode::Regex,
            mode: query.mode,
            matcher,
            failure: Arc::default(),
        })
    }

    pub fn is_regex(&self) -> bool {
        matches!(self.matcher, Matcher::Regex(_))
    }

    /// Why the last search found nothing although it should have looked further: a regular
    /// expression that failed to match. `None` after a search that simply found nothing.
    pub fn failure(&self) -> Option<SearchError> {
        self.failure.lock().ok().and_then(|failure| failure.clone())
    }

    /// The first match starting at or after `from` and ending at or before `to`.
    pub fn find_in(&self, text: &Rope, range: Range<usize>) -> Option<Range<usize>> {
        if let Matcher::Regex(regex) = &self.matcher {
            let haystack = haystack(text);
            let mut found = None;
            self.regex_matches(regex, &haystack, range, |m| {
                found = Some(m);
                false
            });
            return found;
        }
        let mut start = range.start;
        loop {
            let found = match &self.matcher {
                Matcher::Exact { needle } => find_exact(text, needle, start..range.end)?,
                Matcher::Folded { needle } => find_folded(text, needle, start..range.end)?,
                Matcher::Regex(_) => unreachable!(),
            };
            if !self.whole_word || is_whole_word(text, &found) {
                return Some(found);
            }
            start = crate::motion::next_boundary(text, found.start);
        }
    }

    /// The last match inside `range`.
    pub fn find_last_in(&self, text: &Rope, range: Range<usize>) -> Option<Range<usize>> {
        if let Matcher::Regex(regex) = &self.matcher {
            let haystack = haystack(text);
            let mut last = None;
            self.regex_matches(regex, &haystack, range, |m| {
                last = Some(m);
                true
            });
            return last;
        }
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
        if self.is_regex() {
            return self.find_all_in(text, 0..text.len());
        }
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
        if let Matcher::Regex(regex) = &self.matcher {
            let haystack = haystack(text);
            let mut matches = Vec::new();
            self.regex_matches(regex, &haystack, range, |m| {
                matches.push(m);
                true
            });
            return matches;
        }
        let mut scan = Scan::new(range);
        scan.run(self, text, || false);
        scan.into_matches()
    }

    /// Whether `range` of `text` is a match (to decide what "Replace" replaces).
    pub fn is_match(&self, text: &Rope, range: Range<usize>) -> bool {
        if let Matcher::Regex(regex) = &self.matcher {
            let haystack = haystack(text);
            return self.regex_match_at(regex, &haystack, range.clone()) == Some(range);
        }
        self.find_in(text, range.clone()) == Some(range)
    }

    /// What replaces `range`, a match in `text`: `template` as written (Normal mode), with its
    /// escapes (Extended), or with its references to groups expanded (regular expressions, see
    /// [`expand_replacement`]).
    pub fn replacement(&self, text: &Rope, range: Range<usize>, template: &str) -> String {
        match &self.matcher {
            Matcher::Regex(regex) => {
                let haystack = haystack(text);
                let input = RegexInput::new(haystack.as_str())
                    .range(range.clone())
                    .from_pos(range.start)
                    .anchored(true);
                match regex.captures_input(input) {
                    Ok(Some(captures)) => expand_with(&captures, template),
                    _ => String::new(),
                }
            }
            _ => self.literal_replacement(template),
        }
    }

    /// Every match inside `range` with what replaces it, for Replace All.
    pub fn replacements(
        &self,
        text: &Rope,
        range: Range<usize>,
        template: &str,
    ) -> Vec<(Range<usize>, String)> {
        let Matcher::Regex(regex) = &self.matcher else {
            let replacement = self.literal_replacement(template);
            return self
                .find_all_in(text, range)
                .into_iter()
                .map(|found| (found, replacement.clone()))
                .collect();
        };
        let haystack = haystack(text);
        let mut found = Vec::new();
        self.regex_matches(regex, &haystack, range.clone(), |m| {
            found.push(m);
            true
        });
        found
            .into_iter()
            .map(|m| {
                let input = RegexInput::new(haystack.as_str())
                    .range(m.clone())
                    .from_pos(m.start)
                    .anchored(true);
                let replacement = match regex.captures_input(input) {
                    Ok(Some(captures)) => expand_with(&captures, template),
                    _ => String::new(),
                };
                (m, replacement)
            })
            .collect()
    }

    fn literal_replacement(&self, template: &str) -> String {
        if self.mode == SearchMode::Extended {
            unescape_extended(template)
        } else {
            template.to_owned()
        }
    }

    /// Calls `found` with each match of `regex` in `range` of `haystack`, in order and not
    /// overlapping, until it returns false. An empty match right where the previous match
    /// ended is skipped, as the `regex` crate does: `a*` in `aab` matches `aa` and the empty
    /// text at the end, not also the empty text between.
    fn regex_matches(
        &self,
        regex: &Regex,
        haystack: &str,
        range: Range<usize>,
        mut found: impl FnMut(Range<usize>) -> bool,
    ) {
        self.set_failure(None);
        let range = range.start.min(haystack.len())..range.end.min(haystack.len());
        let mut pos = range.start;
        let mut previous_end = None;
        while pos <= range.end {
            let input = RegexInput::new(haystack).range(range.clone()).from_pos(pos);
            match regex.find_input(input) {
                Ok(Some(m)) => {
                    let m = m.start()..m.end();
                    if m.is_empty() && previous_end == Some(m.start) {
                        pos = next_char(haystack, m.start);
                        continue;
                    }
                    if !found(m.clone()) {
                        return;
                    }
                    previous_end = Some(m.end);
                    pos = if m.is_empty() {
                        next_char(haystack, m.end)
                    } else {
                        m.end
                    };
                }
                Ok(None) => return,
                Err(error) => {
                    self.set_failure(Some(SearchError::RegexFailed(error.to_string())));
                    return;
                }
            }
            if pos > haystack.len() {
                return;
            }
        }
    }

    /// The match of `regex` that starts exactly at `range.start`, within `range`.
    fn regex_match_at(
        &self,
        regex: &Regex,
        haystack: &str,
        range: Range<usize>,
    ) -> Option<Range<usize>> {
        self.set_failure(None);
        let input = RegexInput::new(haystack)
            .range(range.clone())
            .from_pos(range.start)
            .anchored(true);
        match regex.find_input(input) {
            Ok(found) => found.map(|m| m.start()..m.end()),
            Err(error) => {
                self.set_failure(Some(SearchError::RegexFailed(error.to_string())));
                None
            }
        }
    }

    fn set_failure(&self, failure: Option<SearchError>) {
        if let Ok(mut slot) = self.failure.lock() {
            *slot = failure;
        }
    }

    /// The longest a match can be, in bytes. Case folding may change a character's length
    /// (the Kelvin sign is three bytes, `k` one), so a folded match may be longer than its
    /// pattern.
    fn max_match_len(&self) -> usize {
        match &self.matcher {
            Matcher::Exact { needle } => needle.len(),
            Matcher::Folded { needle } => needle.len() * 4,
            Matcher::Regex(_) => usize::MAX,
        }
    }
}

/// The text as one string, for regular expressions.
fn haystack(text: &Rope) -> String {
    let mut haystack = String::with_capacity(text.len());
    for chunk in text.chunks() {
        haystack.push_str(chunk);
    }
    haystack
}

/// The byte after the character at `pos` (`pos + 1` at the end, to stop an iteration).
fn next_char(haystack: &str, pos: usize) -> usize {
    haystack[pos..]
        .chars()
        .next()
        .map_or(pos + 1, |ch| pos + ch.len_utf8())
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

    /// Finds the matches that start in the next `step` bytes. A regular expression searches
    /// the whole range at once: its matches have no length limit to cut steps by.
    fn step(&mut self, searcher: &Searcher, text: &Rope) {
        if searcher.is_regex() {
            self.matches = searcher.find_all_in(text, self.range.clone());
            self.next = self.range.end;
            return;
        }
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

/// The text an Extended search or replacement stands for, as in Notepad++: `\n`, `\r`, `\t`,
/// `\0` and `\\`; a character by its code as `\xHH` (hexadecimal), `\uHHHH`, `\oNNN` (octal),
/// `\dNNN` (decimal) or `\bNNNNNNNN` (binary). Any other backslash stays as written.
pub fn unescape_extended(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('\\') {
        out.push_str(&rest[..at]);
        rest = &rest[at + 1..];
        let mut chars = rest.chars();
        let Some(kind) = chars.next() else {
            out.push('\\');
            break;
        };
        let code = |digits: usize, radix: u32| -> Option<char> {
            let number = rest.get(1..1 + digits)?;
            if !number.chars().all(|ch| ch.is_digit(radix)) {
                return None;
            }
            char::from_u32(u32::from_str_radix(number, radix).ok()?)
        };
        let (ch, used) = match kind {
            'n' => (Some('\n'), 1),
            'r' => (Some('\r'), 1),
            't' => (Some('\t'), 1),
            '0' => (Some('\0'), 1),
            '\\' => (Some('\\'), 1),
            'x' => (code(2, 16), 3),
            'u' => (code(4, 16), 5),
            'o' => (code(3, 8), 4),
            'd' => (code(3, 10), 4),
            'b' => (code(8, 2), 9),
            _ => (None, 0),
        };
        match ch {
            Some(ch) => {
                out.push(ch);
                rest = &rest[used..];
            }
            None => out.push('\\'),
        }
    }
    out.push_str(rest);
    out
}

/// Expands a regular expression replacement, as Notepad++ (Boost) does:
///
/// - `$&` or `$0` the whole match, `$1`…`$99` or `\1`…`\9` a group, `${2}` or `${name}` a
///   group by number or name, `$$` a dollar sign;
/// - `\n`, `\r`, `\t`, `\\` a line feed, carriage return, tab and backslash;
/// - `\U` and `\L` turn what follows to uppercase or lowercase until `\E`; `\u` and `\l` only
///   the next character.
///
/// A group that did not take part in the match is empty. Anything else is literal.
pub fn expand_replacement<'a>(
    template: &str,
    group: impl Fn(usize) -> Option<&'a str>,
    named: impl Fn(&str) -> Option<&'a str>,
) -> String {
    #[derive(Clone, Copy, PartialEq)]
    enum Case {
        Keep,
        Upper,
        Lower,
    }
    let mut out = String::new();
    let mut case = Case::Keep;
    // `\u` or `\l`: the next character only.
    let mut next: Option<Case> = None;
    let push = |out: &mut String, text: &str, case: Case, next: &mut Option<Case>| {
        for ch in text.chars() {
            let applied = next.take().unwrap_or(case);
            match applied {
                Case::Keep => out.push(ch),
                Case::Upper => out.extend(ch.to_uppercase()),
                Case::Lower => out.extend(ch.to_lowercase()),
            }
        }
    };
    let digits = |s: &str, max: usize| -> usize {
        s.bytes()
            .take(max)
            .take_while(|b| b.is_ascii_digit())
            .count()
    };
    let mut rest = template;
    while let Some(ch) = rest.chars().next() {
        match ch {
            '$' => {
                let after = &rest[1..];
                if let Some(stripped) = after.strip_prefix('$') {
                    push(&mut out, "$", case, &mut next);
                    rest = stripped;
                } else if let Some(stripped) = after.strip_prefix('&') {
                    push(&mut out, group(0).unwrap_or_default(), case, &mut next);
                    rest = stripped;
                } else if let Some(inner) = after.strip_prefix('{')
                    && let Some(close) = inner.find('}')
                {
                    let name = &inner[..close];
                    let value = match name.parse::<usize>() {
                        Ok(index) => group(index),
                        Err(_) => named(name),
                    };
                    push(&mut out, value.unwrap_or_default(), case, &mut next);
                    rest = &inner[close + 1..];
                } else {
                    let count = digits(after, 2);
                    if count > 0 {
                        let index: usize = after[..count].parse().unwrap_or(0);
                        push(&mut out, group(index).unwrap_or_default(), case, &mut next);
                        rest = &after[count..];
                    } else {
                        push(&mut out, "$", case, &mut next);
                        rest = after;
                    }
                }
            }
            '\\' => {
                let after = &rest[1..];
                let Some(kind) = after.chars().next() else {
                    push(&mut out, "\\", case, &mut next);
                    break;
                };
                rest = &after[kind.len_utf8()..];
                match kind {
                    'n' => push(&mut out, "\n", Case::Keep, &mut None),
                    'r' => push(&mut out, "\r", Case::Keep, &mut None),
                    't' => push(&mut out, "\t", Case::Keep, &mut None),
                    'U' => case = Case::Upper,
                    'L' => case = Case::Lower,
                    'E' => case = Case::Keep,
                    'u' => next = Some(Case::Upper),
                    'l' => next = Some(Case::Lower),
                    '0'..='9' => {
                        let index = kind as usize - '0' as usize;
                        push(&mut out, group(index).unwrap_or_default(), case, &mut next);
                    }
                    other => {
                        let mut buffer = [0; 4];
                        push(&mut out, other.encode_utf8(&mut buffer), case, &mut next);
                    }
                }
            }
            _ => {
                let len = ch.len_utf8();
                push(&mut out, &rest[..len], case, &mut next);
                rest = &rest[len..];
            }
        }
    }
    out
}

fn expand_with(captures: &fancy_regex::Captures<'_, str>, template: &str) -> String {
    expand_replacement(
        template,
        |index| captures.get(index).map(|m| m.as_str()),
        |name| captures.name(name).map(|m| m.as_str()),
    )
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::single_range_in_vec_init,
        reason = "lists of one expected match"
    )]

    use super::*;

    fn searcher(pattern: &str, match_case: bool, whole_word: bool) -> Searcher {
        Searcher::new(&Query {
            pattern: pattern.into(),
            match_case,
            whole_word,
            ..Query::default()
        })
        .unwrap()
    }

    fn mode(pattern: &str, mode: SearchMode) -> Searcher {
        Searcher::new(&Query {
            pattern: pattern.into(),
            match_case: true,
            mode,
            ..Query::default()
        })
        .unwrap()
    }

    fn regex(pattern: &str) -> Searcher {
        mode(pattern, SearchMode::Regex)
    }

    fn matched(searcher: &Searcher, text: &Rope) -> Vec<String> {
        searcher
            .find_all(text)
            .into_iter()
            .map(|range| text.slice(range).to_string())
            .collect()
    }

    #[test]
    fn extended_mode_reads_notepad_plus_plus_escapes() {
        assert_eq!(
            unescape_extended(r"a\tb\nc\\d\x41Ж\o101\d065\b01000001\q\"),
            "a\tb\nc\\dAЖAAA\\q\\"
        );
        assert_eq!(
            unescape_extended(r"\x4"),
            r"\x4",
            "too few digits stay as written"
        );
        let text = Rope::from_str("one\r\ntwo\tthree");
        let s = mode(r"\r\ntwo\t", SearchMode::Extended);
        assert_eq!(s.find_all(&text), [3..9]);
        assert_eq!(s.replacement(&text, 3..9, r"\n-"), "\n-");
        assert_eq!(
            mode(r"\r\n", SearchMode::Normal).find_all(&text),
            Vec::<Range<usize>>::new(),
            "Normal mode is literal"
        );
    }

    #[test]
    fn regular_expressions_have_back_references_and_look_around() {
        let text = Rope::from_str("the the cat sat on on the mat\nprice: $42, cost: $7");
        assert_eq!(
            matched(&regex(r"\b(\w+) \1\b"), &text),
            ["the the", "on on"]
        );
        assert_eq!(matched(&regex(r"(?<=\$)\d+"), &text), ["42", "7"]);
        assert_eq!(matched(&regex(r"\w+(?=:)"), &text), ["price", "cost"]);
        let insensitive = Searcher::new(&Query {
            pattern: "ПРИВЕТ|CAT".into(),
            mode: SearchMode::Regex,
            ..Query::default()
        })
        .unwrap();
        assert_eq!(
            matched(&insensitive, &Rope::from_str("привет, Cat")),
            ["привет", "Cat"]
        );
    }

    #[test]
    fn regular_expressions_work_line_by_line_with_any_line_ending() {
        let text = Rope::from_str("a1\r\nb2\r\nc3");
        assert_eq!(matched(&regex(r"^\w"), &text), ["a", "b", "c"]);
        assert_eq!(matched(&regex(r"\d$"), &text), ["1", "2", "3"]);
        assert_eq!(matched(&regex(r"1.+b"), &text), Vec::<String>::new());
        let across = Searcher::new(&Query {
            pattern: r"1.+b".into(),
            match_case: true,
            mode: SearchMode::Regex,
            dot_matches_newline: true,
            ..Query::default()
        })
        .unwrap();
        assert_eq!(matched(&across, &text), ["1\r\nb"]);
    }

    #[test]
    fn empty_matches_replace_like_notepad_plus_plus() {
        // Commenting out every line: `^` matches at each line start, the empty last line too.
        let text = Rope::from_str("x\ny\n");
        let replaced: Vec<(Range<usize>, String)> =
            regex("^").replacements(&text, 0..text.len(), "// ");
        assert_eq!(
            replaced,
            [
                (0..0, "// ".to_owned()),
                (2..2, "// ".to_owned()),
                (4..4, "// ".to_owned())
            ]
        );
        // No empty match right where a match ended.
        assert_eq!(regex("a*").find_all(&Rope::from_str("aab")), [0..2, 3..3]);
    }

    #[test]
    fn replacements_expand_groups_and_case() {
        let group = |i: usize| ["me@host", "me", "host"].get(i).copied();
        let named = |name: &str| (name == "user").then_some("me");
        let expand = |template: &str| expand_replacement(template, group, named);
        assert_eq!(expand(r"$2 at ${1}"), "host at me");
        assert_eq!(expand(r"\2 \1 $& $0"), "host me me@host me@host");
        assert_eq!(expand(r"${user}!"), "me!");
        assert_eq!(expand(r"\U$1\E-$2 \u$2 \L\uHELLO"), "ME-host Host Hello");
        assert_eq!(
            expand(r"$$5 $9 \t\\"),
            "$5  \t\\",
            "a missing group is empty"
        );
        let text = Rope::from_str("me@host, you@there");
        let s = regex(r"(?<user>\w+)@(\w+)");
        assert_eq!(
            s.replacements(&text, 0..text.len(), r"${user} at \U$2"),
            [
                (0..7, "me at HOST".to_owned()),
                (9..18, "you at THERE".to_owned())
            ]
        );
        assert_eq!(s.replacement(&text, 9..18, "[$1]"), "[you]");
    }

    #[test]
    fn regular_expressions_in_a_selection_see_the_text_around_it() {
        let text = Rope::from_str("a1 b2 c3");
        let s = regex(r"\w\d");
        assert_eq!(s.find_all_in(&text, 3..5), [3..5]);
        assert_eq!(
            s.find_all_in(&text, 3..4),
            Vec::<Range<usize>>::new(),
            "a match must end in the range"
        );
        // Look-behind sees what precedes the selection.
        assert_eq!(regex(r"(?<=1 )\w").find_all_in(&text, 3..8), [3..4]);
        assert_eq!(s.find(&text, 5, Direction::Backward, false), Some(3..5));
        assert_eq!(s.find(&text, 1, Direction::Backward, true), Some(6..8));
        assert!(regex(r"\d+").is_match(&Rope::from_str("ab123"), 2..5));
        assert!(!regex(r"\d+").is_match(&Rope::from_str("ab123"), 1..5));
        let mut scan = Scan::with_step(0..text.len(), 2);
        while !scan.run(&s, &text, || true) {}
        assert_eq!(scan.matches(), s.find_all(&text).as_slice());
    }

    #[test]
    fn invalid_regular_expressions_are_errors() {
        let error = Searcher::new(&Query {
            pattern: "(unclosed".into(),
            mode: SearchMode::Regex,
            ..Query::default()
        })
        .unwrap_err();
        assert!(matches!(error, SearchError::InvalidRegex(_)), "{error}");
        assert_eq!(regex("a").failure(), None);
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
        // In every mode and with match case: an Extended pattern of nothing is empty too.
        for mode in [SearchMode::Normal, SearchMode::Extended] {
            for match_case in [false, true] {
                let query = Query {
                    mode,
                    match_case,
                    ..Query::default()
                };
                assert_eq!(Searcher::new(&query).unwrap_err(), SearchError::Empty);
            }
        }
    }

    /// A pattern fancy-regex can only run by backtracking (a back-reference), on a text where
    /// that takes longer than the limit allows.
    fn runaway() -> (Searcher, Rope) {
        let text = Rope::from_str(&format!("{}!", "a".repeat(64)));
        (regex(r"^(a|a)*\1b"), text)
    }

    #[test]
    fn a_runaway_regular_expression_fails_instead_of_finding_nothing() {
        let (s, text) = runaway();
        assert_eq!(s.failure(), None, "nothing has run yet");
        assert_eq!(s.find_in(&text, 0..text.len()), None);
        assert!(matches!(s.failure(), Some(SearchError::RegexFailed(_))));
        assert_eq!(s.find_last_in(&text, 0..text.len()), None);
        assert!(s.failure().is_some());
        assert!(s.replacements(&text, 0..text.len(), "x").is_empty());
        assert!(!s.is_match(&text, 0..text.len()));
        // A search that ends normally clears the failure.
        let fine = regex("a");
        assert!(fine.find_in(&text, 0..text.len()).is_some());
        assert_eq!(fine.failure(), None);
    }

    #[test]
    fn searching_backward_finds_a_match_far_before_the_end() {
        // The backward search looks in growing windows from the end: the only match is at
        // the very start, many windows away; another is cut by the end of the range.
        let source = format!("needle{}needle", "-".repeat(100_000));
        let text = Rope::from_str(&source);
        let s = searcher("needle", true, false);
        assert_eq!(s.find_last_in(&text, 0..text.len() - 1), Some(0..6));
        assert_eq!(s.find_last_in(&text, 1..text.len() - 1), None);
        assert_eq!(s.find_last_in(&text, 0..0), None, "an empty range");
        assert_eq!(
            s.find_last_in(&text, 0..text.len()),
            Some(text.len() - 6..text.len())
        );
    }

    #[test]
    fn extended_escapes_at_their_limits() {
        // A backslash at the very end, codes with too few or wrong digits, and the largest
        // and smallest codes.
        assert_eq!(unescape_extended("end\\"), "end\\");
        assert_eq!(unescape_extended(r"\u41"), r"\u41");
        assert_eq!(unescape_extended(r"\xZZ"), r"\xZZ");
        assert_eq!(unescape_extended(r"\o9"), r"\o9");
        assert_eq!(unescape_extended(r"\d25"), r"\d25");
        assert_eq!(unescape_extended(r"\xff\x00"), "\u{ff}\0");
        assert_eq!(unescape_extended(r"\uffff"), "\u{ffff}");
        assert_eq!(unescape_extended(r"\o377\d255"), "\u{ff}\u{ff}");
        // A surrogate is no character: it stays as written.
        assert_eq!(unescape_extended(r"\ud800"), r"\ud800");
        assert_eq!(unescape_extended(""), "");
    }

    #[test]
    fn replacement_templates_at_their_limits() {
        let group = |i: usize| ["whole", "one"].get(i).copied();
        let named = |_: &str| None;
        let expand = |template: &str| expand_replacement(template, group, named);
        // A `$` or a backslash at the very end is kept; `$` before a non-group is literal.
        assert_eq!(expand("cost $"), "cost $");
        assert_eq!(expand("path\\"), "path\\");
        assert_eq!(expand("$x"), "$x");
        // An unknown name, an unclosed name, group 0 and the empty template.
        assert_eq!(expand("${nope}"), "");
        assert_eq!(expand("${one"), "${one");
        assert_eq!(expand(r"\0"), "whole");
        assert_eq!(expand(""), "");
        // Line breaks are not case-converted, and \E ends a conversion.
        assert_eq!(expand(r"\Ua\nb\Ec"), "A\nBc");
        assert_eq!(expand(r"\l\U$1"), "oNE");
    }

    /// Where `needle` occurs in `haystack`, without overlaps, by the plainest means.
    fn naive(haystack: &str, needle: &str, match_case: bool) -> Vec<Range<usize>> {
        let (haystack, needle) = if match_case {
            (haystack.to_owned(), needle.to_owned())
        } else {
            (haystack.to_lowercase(), needle.to_lowercase())
        };
        haystack
            .match_indices(&needle)
            .map(|(at, found)| at..at + found.len())
            .collect()
    }

    proptest::proptest! {
        #[test]
        fn normal_search_finds_what_a_plain_search_finds(
            pieces in proptest::collection::vec("[abAB \n]{0,40}", 1..60),
            needle in "[abAB]{1,4}",
            match_case: bool,
        ) {
            // Built from pieces so that chunk boundaries fall all over the text.
            let source = pieces.concat();
            let mut text = Rope::new();
            for piece in &pieces {
                let end = text.len();
                text.insert(end, piece);
            }
            let found = searcher(&needle, match_case, false).find_all(&text);
            proptest::prop_assert_eq!(found, naive(&source, &needle, match_case));
        }

        #[test]
        fn a_backward_search_finds_the_last_forward_match(
            source in "[ab \n]{0,300}",
            needle in "[ab]{1,3}",
        ) {
            let text = Rope::from_str(&source);
            let s = searcher(&needle, true, false);
            let all = s.find_all(&text);
            let last = s.find_last_in(&text, 0..text.len());
            // The last match backward may overlap the last forward one ("aa" in "aaa").
            match (all.last(), last) {
                (None, None) => {}
                (Some(forward), Some(backward)) => {
                    proptest::prop_assert!(backward.start >= forward.start);
                    proptest::prop_assert_eq!(&source[backward.clone()], needle.as_str());
                }
                (forward, backward) => proptest::prop_assert!(
                    false,
                    "forward {:?}, backward {:?}",
                    forward,
                    backward
                ),
            }
        }
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
