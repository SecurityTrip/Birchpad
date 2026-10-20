//! Clickable links (Notepad++'s Preferences > Cloud & Link) and Ctrl+double click selection
//! between delimiters (Preferences > Delimiter), as pure functions of the text.

use std::ops::Range;

use ropey::Rope;

use crate::motion::{line_of, line_range};

/// The schemes every link may start with; Preferences add their own.
pub const SCHEMES: &[&str] = &[
    "https://", "http://", "ftps://", "ftp://", "file://", "mailto:",
];

/// The addresses in `line`: a scheme of `SCHEMES` or `extra` (compared without regard to
/// case) with at least one character after it, up to a space, a quote or `<>`. Punctuation at
/// the end (`.,;:!?'`) is left out, and so is a closing bracket that has no opening one in the
/// address.
pub fn links_in(line: &str, extra: &[String]) -> Vec<Range<usize>> {
    let lower = line.to_ascii_lowercase();
    let schemes = SCHEMES
        .iter()
        .copied()
        .chain(extra.iter().map(String::as_str))
        .filter(|scheme| !scheme.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    let mut links = Vec::new();
    let mut at = 0;
    while at < line.len() {
        let found = schemes
            .iter()
            .filter_map(|scheme| {
                lower[at..]
                    .find(scheme.as_str())
                    .map(|i| (at + i, scheme.len()))
            })
            .min_by_key(|(start, len)| (*start, std::cmp::Reverse(*len)));
        let Some((start, scheme_len)) = found else {
            break;
        };
        // A scheme inside a word (`xhttp://`) is not one.
        let inside_word = line[..start]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric());
        let rest = &line[start + scheme_len..];
        let length = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '<' | '>' | '`'))
            .unwrap_or(rest.len());
        let mut end = start + scheme_len + length;
        loop {
            let body = &line[start..end];
            let Some(last) = body.chars().next_back() else {
                break;
            };
            let unmatched = |open: char, close: char| {
                last == close && body.matches(close).count() > body.matches(open).count()
            };
            if matches!(last, '.' | ',' | ';' | ':' | '!' | '?' | '\'')
                || unmatched('(', ')')
                || unmatched('[', ']')
                || unmatched('{', '}')
            {
                end -= last.len_utf8();
            } else {
                break;
            }
        }
        if !inside_word && end > start + scheme_len {
            links.push(start..end);
        }
        at = end.max(start + scheme_len);
    }
    links
}

/// The link at byte `pos` of `text`, if there is one.
pub fn link_at(text: &Rope, pos: usize, extra: &[String]) -> Option<Range<usize>> {
    let pos = pos.min(text.len());
    let line = line_range(text, line_of(text, pos));
    let content = text.slice(line.clone()).to_string();
    links_in(&content, extra)
        .into_iter()
        .map(|range| line.start + range.start..line.start + range.end)
        .find(|range| range.start <= pos && pos <= range.end)
}

/// Ctrl+double click: the text between the nearest `open` before `pos` and the nearest
/// `close` after it, on its line unless `multi_line`. `None` if either is missing.
pub fn between_delimiters(
    text: &Rope,
    pos: usize,
    open: char,
    close: char,
    multi_line: bool,
) -> Option<Range<usize>> {
    let pos = text.floor_char_boundary(pos.min(text.len()));
    let line = line_range(text, line_of(text, pos));
    let (from, to) = if multi_line {
        (0, text.len())
    } else {
        (line.start, line.end)
    };
    let before = text.slice(from..pos).to_string();
    let start = from + before.rfind(open)? + open.len_utf8();
    let after = text.slice(pos..to).to_string();
    let end = pos + after.find(close)?;
    Some(start..end)
}
