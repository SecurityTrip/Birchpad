//! Fuzzy matching, as the command palette and the file finder do it: the characters of what is
//! typed appear in a text in order, not necessarily side by side, without regard to case.
//!
//! What is typed is split at spaces into terms, and each term must match on its own, so that
//! `app work` finds `crates/app/src/workspace.rs`. A match scores higher when its characters
//! start words (after a space, `/`, `\`, `_`, `-`, `.` or `:`, or at a capital inside a word),
//! follow each other, and come with fewer characters skipped between them.

/// How a text matches what was typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// Higher is better; only comparable between matches of the same query.
    pub score: i32,
    /// The byte offsets in the text of the characters matched, ascending, each once.
    pub positions: Vec<usize>,
}

/// A character matched at the start of a word.
const WORD_START: i32 = 8;
/// A capital after a small letter (`camelCase`).
const CAPITAL: i32 = 6;
/// A character right after the one matched before it.
const ADJACENT: i32 = 5;
/// The same case as typed.
const SAME_CASE: i32 = 1;
/// Each character every matched one is worth.
const MATCHED: i32 = 16;
/// Texts longer than this are matched without looking for the best alignment.
const ALIGN_LIMIT: usize = 1 << 16;

/// Whether `c` separates words.
fn is_separator(c: char) -> bool {
    c.is_whitespace() || matches!(c, '/' | '\\' | '_' | '-' | '.' | ':' | ',' | '(' | ')')
}

/// The characters of a text with their byte offsets, case and the bonus each would earn.
struct Chars {
    lower: Vec<char>,
    original: Vec<char>,
    offsets: Vec<usize>,
    bonus: Vec<i32>,
}

impl Chars {
    fn new(text: &str) -> Self {
        let mut chars = Self {
            lower: Vec::new(),
            original: Vec::new(),
            offsets: Vec::new(),
            bonus: Vec::new(),
        };
        let mut previous: Option<char> = None;
        for (offset, c) in text.char_indices() {
            let bonus = match previous {
                None => WORD_START,
                Some(before) if is_separator(before) && !is_separator(c) => WORD_START,
                Some(before) if before.is_lowercase() && c.is_uppercase() => CAPITAL,
                Some(before) if !before.is_alphanumeric() && c.is_alphanumeric() => CAPITAL,
                _ => 0,
            };
            chars.lower.push(lower(c));
            chars.original.push(c);
            chars.offsets.push(offset);
            chars.bonus.push(bonus);
            previous = Some(c);
        }
        chars
    }
}

/// `c` in small letters, one character for one.
fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// How `query` matches `text`: none if a term of it does not. An empty query (or one of
/// spaces only) matches every text, with no characters.
pub fn find(query: &str, text: &str) -> Option<Match> {
    let terms: Vec<&str> = query.split_whitespace().collect();
    if terms.is_empty() {
        return Some(Match {
            score: 0,
            positions: Vec::new(),
        });
    }
    let chars = Chars::new(text);
    let mut score = 0;
    let mut positions = Vec::new();
    for term in terms {
        let term: Vec<char> = term.chars().collect();
        let (term_score, found) = align(&term, &chars)?;
        score += term_score;
        positions.extend(found.into_iter().map(|index| chars.offsets[index]));
    }
    positions.sort_unstable();
    positions.dedup();
    Some(Match { score, positions })
}

/// What a character of the term earns at `index` of the text, the step from before aside.
fn worth(term: char, chars: &Chars, index: usize) -> i32 {
    MATCHED
        + chars.bonus[index]
        + if chars.original[index] == term {
            SAME_CASE
        } else {
            0
        }
}

/// The best alignment of `term` in the text: its score and the indexes of the characters.
fn align(term: &[char], chars: &Chars) -> Option<(i32, Vec<usize>)> {
    let n = chars.lower.len();
    let m = term.len();
    // The earliest place each character of the term can be: a quick no, and where to start.
    let mut first = Vec::with_capacity(m);
    let mut from = 0;
    for &c in term {
        let c = lower(c);
        let at = (from..n).find(|&index| chars.lower[index] == c)?;
        first.push(at);
        from = at + 1;
    }
    if n.saturating_mul(m) > ALIGN_LIMIT {
        let score = first
            .iter()
            .zip(term)
            .map(|(&index, &c)| worth(c, chars, index))
            .sum();
        return Some((score, first));
    }
    const NONE: i32 = i32::MIN / 2;
    // best[i][j]: the best score of the term's first i + 1 characters with the last at j.
    let mut best = vec![NONE; m * n];
    // back[i][j]: where the term's character i - 1 is in that alignment.
    let mut back = vec![usize::MAX; m * n];
    for i in 0..m {
        let c = lower(term[i]);
        // The best of best[i - 1][k] for k < j - 1, less a point for each character skipped
        // since, and its k.
        let mut gap = (NONE, usize::MAX);
        for j in 0..n {
            if i > 0 && j >= 2 {
                let candidate = best[(i - 1) * n + j - 2];
                gap = if candidate > gap.0 {
                    (candidate - 1, j - 2)
                } else {
                    (gap.0 - 1, gap.1)
                };
            }
            if j < first[i] || chars.lower[j] != c {
                continue;
            }
            let here = worth(term[i], chars, j);
            if i == 0 {
                best[j] = here;
                continue;
            }
            let adjacent = if j >= 1 {
                best[(i - 1) * n + j - 1]
            } else {
                NONE
            };
            let (previous, at) = if adjacent > NONE && adjacent + ADJACENT >= gap.0 {
                (adjacent + ADJACENT, j - 1)
            } else {
                gap
            };
            if previous > NONE {
                best[i * n + j] = previous + here;
                back[i * n + j] = at;
            }
        }
    }
    let last = (m - 1) * n;
    let (end, score) = (0..n)
        .map(|j| (j, best[last + j]))
        .filter(|&(_, score)| score > NONE)
        // The first of equal scores: the earliest match.
        .fold(
            None,
            |found: Option<(usize, i32)>, (j, score)| match found {
                Some((_, kept)) if kept >= score => found,
                _ => Some((j, score)),
            },
        )?;
    let mut indexes = vec![end; m];
    for i in (1..m).rev() {
        indexes[i - 1] = back[i * n + indexes[i]];
    }
    Some((score, indexes))
}

/// The texts `query` matches, best first: by score, then shorter texts, then in the order
/// given; with nothing typed, every text in the order given. Each comes with its index in
/// `texts`.
pub fn rank<'a>(query: &str, texts: impl IntoIterator<Item = &'a str>) -> Vec<(usize, Match)> {
    if query.split_whitespace().next().is_none() {
        return texts
            .into_iter()
            .enumerate()
            .map(|(index, _)| {
                (
                    index,
                    Match {
                        score: 0,
                        positions: Vec::new(),
                    },
                )
            })
            .collect();
    }
    let mut found: Vec<(usize, usize, Match)> = texts
        .into_iter()
        .enumerate()
        .filter_map(|(index, text)| find(query, text).map(|found| (index, text.len(), found)))
        .collect();
    found.sort_by(|(a, a_len, a_match), (b, b_len, b_match)| {
        b_match
            .score
            .cmp(&a_match.score)
            .then(a_len.cmp(b_len))
            .then(a.cmp(b))
    });
    found
        .into_iter()
        .map(|(index, _, found)| (index, found))
        .collect()
}

/// As [`rank`], but the first `recent` texts, the recently used ones in the order they were
/// used, come first when they match, in that order; the others follow, best first.
pub fn rank_recent<'a>(
    query: &str,
    texts: impl IntoIterator<Item = &'a str>,
    recent: usize,
) -> Vec<(usize, Match)> {
    let (mut first, rest): (Vec<_>, Vec<_>) = rank(query, texts)
        .into_iter()
        .partition(|(index, _)| *index < recent);
    first.sort_by_key(|(index, _)| *index);
    first.extend(rest);
    first
}
