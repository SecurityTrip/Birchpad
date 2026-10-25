//! Selecting by the syntax tree: the smallest piece of syntax around a selection, for Expand
//! Selection. A node with brackets or quotes at both ends (`(a, b)`, `"text"`) offers what is
//! between them first, then itself.

use std::ops::Range;

use tree_sitter::{Node, Tree};

use crate::Syntax;

/// The smallest range of syntax larger than `range` that holds it, in the document's tree or
/// the trees of the languages embedded where `range` is; none when no node is larger.
pub fn expand_selection(syntax: &Syntax, range: Range<usize>) -> Option<Range<usize>> {
    let trees = syntax.tree().into_iter().chain(
        syntax
            .layers()
            .iter()
            .filter(|layer| layer.covers(range.start) && layer.covers(range.end))
            .map(crate::Layer::tree),
    );
    trees
        .filter_map(|tree| expand_in(tree, range.clone()))
        .min_by_key(|found| found.end - found.start)
}

/// Whether `outer` holds `inner` and is larger.
fn holds(outer: &Range<usize>, inner: &Range<usize>) -> bool {
    outer.start <= inner.start && inner.end <= outer.end && outer.len() > inner.len()
}

fn expand_in(tree: &Tree, range: Range<usize>) -> Option<Range<usize>> {
    let mut node = tree
        .root_node()
        .descendant_for_byte_range(range.start, range.end)?;
    loop {
        if node.is_named() || node.parent().is_none() {
            if let Some(inner) = inside_delimiters(node)
                && holds(&inner, &range)
            {
                return Some(inner);
            }
            if holds(&node.byte_range(), &range) {
                return Some(node.byte_range());
            }
        }
        node = node.parent()?;
    }
}

/// Tokens that open what [`inside_delimiters`] selects the inside of, and those that close it.
const OPENING: &[&str] = &["(", "[", "{", "<", "\"", "'", "`", "\"\"\"", "'''"];
const CLOSING: &[&str] = &[")", "]", "}", ">", "\"", "'", "`", "\"\"\"", "'''"];

/// What is between a node's first and last child when they are brackets or quotes and
/// something is between them.
fn inside_delimiters(node: Node) -> Option<Range<usize>> {
    let count = node.child_count();
    if count < 2 {
        return None;
    }
    let first = node.child(0)?;
    let last = node.child(count - 1)?;
    let inner = first.end_byte()..last.start_byte();
    let delimits = !first.is_named()
        && !last.is_named()
        && OPENING.contains(&first.kind())
        && CLOSING.contains(&last.kind());
    (delimits && inner.start < inner.end).then_some(inner)
}
