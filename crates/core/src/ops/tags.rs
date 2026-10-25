//! Closing HTML and XML tags as their opening tag is typed (Auto-Completion > Close HTML and XML
//! tags).

use ropey::Rope;

use crate::change::Edit;
use crate::completion::closing_tag;
use crate::selection::{Range as Span, Selection};
use crate::transaction::Transaction;

/// How far back from a caret an opening tag is looked for: a tag longer than this is not
/// closed.
const TAG_REACH: usize = 4096;

/// After `>` was typed at every caret: inserts the closing tag of the opening tag each caret
/// ends, after the caret, which stays where it is. `None` when no caret ends an opening tag
/// (`html` leaves out HTML's void elements).
pub fn close_tags(text: &Rope, selection: &Selection, html: bool) -> Option<Transaction> {
    if selection.iter().any(|range| !range.is_empty()) {
        return None;
    }
    let mut edits = Vec::new();
    let mut carets = Vec::new();
    let mut shift = 0;
    for range in selection.iter() {
        let pos = range.head;
        let mut from = pos.saturating_sub(TAG_REACH);
        while !text.is_char_boundary(from) {
            from += 1;
        }
        let before = text.slice(from..pos).to_string();
        carets.push(Span::point(pos + shift));
        if let Some(tag) = closing_tag(&before, html) {
            shift += tag.len();
            edits.push(Edit::insert(pos, tag));
        }
    }
    if edits.is_empty() {
        return None;
    }
    let transaction = Transaction::from_edits(text, edits).ok()?;
    // The carets stay before the inserted tags.
    Some(transaction.with_selection(Selection::new(carets, selection.primary_index())))
}

/// How far around the caret Highlight Matching Tags looks for the partner of a tag.
pub const MATCH_REACH: usize = 1 << 20;

/// An HTML or XML tag: its whole text (`<a href="x">`) and its name (`a`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub range: std::ops::Range<usize>,
    pub name: std::ops::Range<usize>,
}

/// The tag at a caret and its partner, opening tag first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagPair {
    pub open: Tag,
    pub close: Tag,
}

/// The tags of `text` (from `offset`) in order, with whether each closes (`</a>`); self-closed
/// tags (`<br/>`), declarations (`<!DOCTYPE>`) and processing instructions (`<?xml?>`) are left
/// out, and so are comments and CDATA sections unless `in_comments`.
fn tags(text: &str, offset: usize, in_comments: bool) -> Vec<(Tag, bool)> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(start) = text[at..].find('<').map(|index| at + index) {
        let rest = &text[start..];
        let skip_to = |end: &str| rest.find(end).map(|index| start + index + end.len());
        if rest.starts_with("<!--") || rest.starts_with("<![CDATA[") {
            let end = if rest.starts_with("<!--") {
                "-->"
            } else {
                "]]>"
            };
            let Some(after) = skip_to(end) else { break };
            if in_comments {
                // Tags inside count as any other.
                let inner_start = start + if end == "-->" { 4 } else { 9 };
                let inner = &text[inner_start..after - end.len()];
                found.extend(tags(inner, offset + inner_start, true));
            }
            at = after;
            continue;
        }
        let Some(end) = rest.find('>').map(|index| start + index) else {
            break;
        };
        let closing = bytes.get(start + 1) == Some(&b'/');
        let name_start = start + 1 + usize::from(closing);
        let name_len = text[name_start..end]
            .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
            .unwrap_or(end - name_start);
        let self_closed = bytes[end - 1] == b'/';
        let declaration = matches!(bytes.get(start + 1), Some(b'!' | b'?'));
        if name_len > 0 && !self_closed && !declaration {
            found.push((
                Tag {
                    range: offset + start..offset + end + 1,
                    name: offset + name_start..offset + name_start + name_len,
                },
                closing,
            ));
        }
        at = end + 1;
    }
    found
}

/// Highlight Matching Tags: the tag the caret `pos` is in and its partner, within
/// [`MATCH_REACH`] bytes around it. None for a tag without a partner, a self-closed one, or a
/// caret outside tags.
pub fn matching_tag(text: &Rope, pos: usize, in_comments: bool) -> Option<TagPair> {
    let mut from = pos.saturating_sub(MATCH_REACH);
    while !text.is_char_boundary(from) {
        from += 1;
    }
    let mut to = (pos + MATCH_REACH).min(text.len());
    while !text.is_char_boundary(to) {
        to -= 1;
    }
    let window = text.slice(from..to).to_string();
    let all = tags(&window, from, in_comments);
    let index = all
        .iter()
        .position(|(tag, _)| tag.range.start < pos && pos < tag.range.end)?;
    let (tag, closing) = &all[index];
    let name = |tag: &Tag| &window[tag.name.start - from..tag.name.end - from];
    let wanted = name(tag);
    let mut depth = 0usize;
    if *closing {
        for (other, other_closing) in all[..index].iter().rev() {
            if name(other) != wanted {
                continue;
            }
            if *other_closing {
                depth += 1;
            } else if depth == 0 {
                return Some(TagPair {
                    open: other.clone(),
                    close: tag.clone(),
                });
            } else {
                depth -= 1;
            }
        }
    } else {
        for (other, other_closing) in &all[index + 1..] {
            if name(other) != wanted {
                continue;
            }
            if !*other_closing {
                depth += 1;
            } else if depth == 0 {
                return Some(TagPair {
                    open: tag.clone(),
                    close: other.clone(),
                });
            } else {
                depth -= 1;
            }
        }
    }
    None
}
