//! Brace matching, as Notepad++ does it: the bracket before the caret, else the one after it,
//! and its partner.
//!
//! With a syntax tree, only brackets that are tokens of the language match (a parenthesis in a
//! string or a comment is just text), and partners come from the tree, so they are right even
//! across unbalanced brackets in strings. Plain text falls back to counting brackets.

use std::ops::Range;

use birchpad_core::Rope;

use crate::syntax::Syntax;

/// How far plain-text matching scans for a partner, in bytes.
const SCAN_LIMIT: usize = 1 << 20;

/// A bracket at the caret and its partner, if it has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BracketMatch {
    pub bracket: Range<usize>,
    /// `None` for an unmatched bracket (Notepad++'s "bad brace").
    pub partner: Option<Range<usize>>,
}

fn partner_of(byte: u8) -> Option<(u8, bool)> {
    match byte {
        b'(' => Some((b')', true)),
        b'[' => Some((b']', true)),
        b'{' => Some((b'}', true)),
        b')' => Some((b'(', false)),
        b']' => Some((b'[', false)),
        b'}' => Some((b'{', false)),
        _ => None,
    }
}

/// The bracket next to `caret` (before it first) and its partner.
pub fn matching_bracket(
    text: &Rope,
    syntax: Option<&Syntax>,
    caret: usize,
) -> Option<BracketMatch> {
    let candidates = [caret.checked_sub(1), Some(caret)];
    let at = candidates
        .into_iter()
        .flatten()
        .find(|&pos| pos < text.len() && partner_of(text.byte(pos)).is_some())?;
    match syntax.and_then(Syntax::tree) {
        Some(tree) => tree_match(tree, text, at),
        None => Some(BracketMatch {
            bracket: at..at + 1,
            partner: scan(text, at),
        }),
    }
}

/// The partner of the bracket at `at` from the syntax tree; `None` if that bracket is not a
/// token of the language.
fn tree_match(tree: &tree_sitter::Tree, text: &Rope, at: usize) -> Option<BracketMatch> {
    let byte = text.byte(at);
    let (partner, opening) = partner_of(byte)?;
    let node = tree.root_node().descendant_for_byte_range(at, at + 1)?;
    let is_token =
        !node.is_named() && node.byte_range() == (at..at + 1) && node.kind().as_bytes() == [byte];
    if !is_token {
        return None;
    }
    let parent = node.parent()?;
    let mut cursor = parent.walk();
    let siblings: Vec<_> = parent.children(&mut cursor).collect();
    let index = siblings.iter().position(|sibling| *sibling == node)?;
    let is_partner = |sibling: &&tree_sitter::Node| {
        !sibling.is_named() && sibling.kind().as_bytes() == [partner]
    };
    let found = if opening {
        siblings[index + 1..].iter().find(is_partner)
    } else {
        siblings[..index].iter().rev().find(is_partner)
    };
    Some(BracketMatch {
        bracket: at..at + 1,
        partner: found
            .filter(|sibling| !sibling.is_missing())
            .map(|sibling| sibling.byte_range()),
    })
}

/// The partner of the bracket at `at` by counting brackets, within [`SCAN_LIMIT`] bytes.
fn scan(text: &Rope, at: usize) -> Option<Range<usize>> {
    let byte = text.byte(at);
    let (partner, opening) = partner_of(byte)?;
    let mut depth = 0usize;
    if opening {
        let end = text.len().min(at + SCAN_LIMIT);
        for (offset, current) in text.slice(at + 1..end).bytes().enumerate() {
            if current == byte {
                depth += 1;
            } else if current == partner {
                if depth == 0 {
                    let pos = at + 1 + offset;
                    return Some(pos..pos + 1);
                }
                depth -= 1;
            }
        }
    } else {
        let start = at.saturating_sub(SCAN_LIMIT);
        let mut pos = at;
        let before = text.slice(start..at);
        for current in before.bytes_at(before.len()).reversed() {
            pos -= 1;
            if current == byte {
                depth += 1;
            } else if current == partner {
                if depth == 0 {
                    return Some(pos..pos + 1);
                }
                depth -= 1;
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::by_id;
    use crate::syntax::config;
    use std::sync::atomic::AtomicBool;

    fn rust(source: &str) -> (Rope, Syntax) {
        let text = Rope::from_str(source);
        let mut syntax = Syntax::new(config(by_id("rust").unwrap()).unwrap());
        let tree = syntax
            .parse_job(text.clone())
            .run(&AtomicBool::new(false))
            .unwrap();
        syntax.install(tree);
        (text, syntax)
    }

    #[test]
    fn plain_text_counts_brackets() {
        let text = Rope::from_str("a(b[c]d)e)");
        let at = |caret| matching_bracket(&text, None, caret);
        // Before the caret wins over after it.
        assert_eq!(at(2).unwrap().partner, Some(7..8));
        assert_eq!(at(1).unwrap().bracket, 1..2, "after the caret");
        assert_eq!(at(6).unwrap().partner, Some(3..4));
        assert_eq!(at(10).unwrap().partner, None, "unmatched");
        assert_eq!(at(0), None);
    }

    #[test]
    fn syntax_ignores_brackets_in_strings_and_comments() {
        let source = "fn f() { g(\"(\"); } // (";
        let (text, syntax) = rust(source);
        let at = |caret| matching_bracket(&text, Some(&syntax), caret);
        // The `(` of `g(` matches the `)` after the string, not the one in it.
        let call = source.find("g(").unwrap() + 2;
        let close = source.find(");").unwrap();
        assert_eq!(at(call).unwrap().partner, Some(close..close + 1));
        assert_eq!(
            at(source.find('{').unwrap() + 1)
                .unwrap()
                .partner
                .map(|r| r.start),
            source.rfind('}')
        );
        // A bracket in a string or a comment does not match anything.
        assert_eq!(at(source.find("\"(").unwrap() + 2), None);
        assert_eq!(at(source.len()), None);
    }
}
