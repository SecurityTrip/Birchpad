//! Fold points: from the syntax tree, or from indentation for plain text.
//!
//! Syntax folds need no per-language queries: functions, classes, blocks, objects, arrays,
//! multi-line comments and strings, tags and sections all are multi-line named nodes. When
//! several start on the same line (a function and its body block), the outermost one is the
//! fold, so the header line folds everything that starts there.

use std::ops::Range;

use birchpad_core::Rope;
use birchpad_core::motion::{line_count, line_of, line_range};
use tree_sitter::{Node, Tree};

/// Bodies that start with their first statement or entry rather than a token of their own
/// (Python and Lua blocks, Ruby method bodies, YAML mappings). Their first line is content, so
/// folding there would hide the following siblings; the statements fold on their own instead.
const BODIES: &[&str] = &[
    "block",
    "body_statement",
    "block_node",
    "block_mapping",
    "block_sequence",
];

/// Byte ranges of the foldable nodes, sorted by start, one per starting line. Each range ends
/// on the fold's last line.
pub fn fold_ranges(tree: &Tree, text: &Rope) -> Vec<Range<usize>> {
    let mut folds: Vec<Range<usize>> = Vec::new();
    let mut last_line = None;
    let mut cursor = tree.walk();
    // Preorder: a node comes before its children, and siblings in order, so starts are sorted.
    let mut visit = |node: Node| {
        if !node.is_named() || node.parent().is_none() || is_body(node) {
            return;
        }
        let range = node.byte_range();
        let end = range.end.min(text.len());
        if range.start >= end {
            return;
        }
        let first_line = line_of(text, range.start);
        if line_of(text, last_char(text, end)) <= first_line || last_line == Some(first_line) {
            return;
        }
        last_line = Some(first_line);
        folds.push(range.start..end);
    };
    'walk: loop {
        visit(cursor.node());
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                continue 'walk;
            }
            if !cursor.goto_parent() {
                break 'walk;
            }
        }
    }
    folds
}

fn is_body(node: Node) -> bool {
    BODIES.contains(&node.kind())
        && node
            .named_child(0)
            .is_some_and(|child| child.start_byte() == node.start_byte())
}

/// The position of the last character before `end` (a range ending right after a line break
/// ends on the line of that break).
fn last_char(text: &Rope, end: usize) -> usize {
    if end == 0 {
        0
    } else {
        text.floor_char_boundary(end - 1)
    }
}

/// Folds of plain text by indentation, as in many editors: a line folds the lines after it that
/// are more indented (blank lines inside are included, trailing ones are not). Linear in the
/// number of lines.
pub fn indent_fold_ranges(text: &Rope, tab_width: usize) -> Vec<Range<usize>> {
    let tab_width = tab_width.max(1);
    let mut folds = Vec::new();
    // Lines whose block is still open: (line, indentation).
    let mut open: Vec<(usize, usize)> = Vec::new();
    let mut last_content = 0;
    let mut close = |open: &mut Vec<(usize, usize)>, upto: Option<usize>, last_content: usize| {
        while let Some(&(header, indent)) = open.last() {
            if upto.is_some_and(|current| current > indent) {
                break;
            }
            open.pop();
            if last_content > header {
                folds.push(line_range(text, header).start..line_range(text, last_content).end);
            }
        }
    };
    for line in 0..line_count(text) {
        let mut column = 0;
        let mut blank = true;
        for ch in text.slice(line_range(text, line)).chars() {
            match ch {
                ' ' => column += 1,
                '\t' => column += tab_width - column % tab_width,
                _ => {
                    blank = false;
                    break;
                }
            }
        }
        if blank {
            continue;
        }
        close(&mut open, Some(column), last_content);
        open.push((line, column));
        last_content = line;
    }
    close(&mut open, None, last_content);
    folds.sort_by_key(|range| range.start);
    folds
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::by_id;
    use crate::syntax::{Syntax, config};
    use std::sync::atomic::AtomicBool;

    fn describe(text: &Rope, source: &str, folds: Vec<Range<usize>>) -> Vec<String> {
        folds
            .into_iter()
            .map(|range| {
                let first = source[range.clone()].lines().next().unwrap_or_default();
                let last = line_of(text, last_char(text, range.end));
                format!("{}..{} {first}", line_of(text, range.start), last)
            })
            .collect()
    }

    fn folds(language: &str, source: &str) -> Vec<String> {
        let text = Rope::from_str(source);
        let mut syntax = Syntax::new(config(by_id(language).unwrap()).unwrap());
        let tree = syntax
            .parse_job(text.clone())
            .run(&AtomicBool::new(false))
            .unwrap();
        syntax.install(tree);
        describe(&text, source, fold_ranges(syntax.tree().unwrap(), &text))
    }

    #[test]
    fn multi_line_nodes_fold_once_per_line() {
        let source = "\
/* a
   comment */
fn f() {
    if x {
        y();
    }
    let s = [
        1,
    ];
}
fn g() {}
";
        assert_eq!(
            folds("rust", source),
            [
                "0..1 /* a",
                "2..9 fn f() {",
                "3..5 if x {",
                "6..8 let s = [",
            ]
        );
    }

    #[test]
    fn bodies_without_an_opening_token_do_not_fold_their_first_line() {
        let source =
            "def f():\n    x = 1\n    if x:\n        pass\n    return 1\n\nclass C:\n    pass\n";
        assert_eq!(
            folds("python", source),
            ["0..4 def f():", "2..3 if x:", "6..7 class C:"]
        );
        let yaml = "jobs:\n  test:\n    runs-on: x\n    steps: 1\n  other: 2\n";
        assert_eq!(folds("yaml", yaml), ["0..4 jobs:", "1..3 test:"]);
    }

    #[test]
    fn markup_folds_elements_and_sections() {
        let html = "<ul>\n  <li>a</li>\n  <li>\n    b\n  </li>\n</ul>\n";
        assert_eq!(folds("html", html), ["0..5 <ul>", "2..4 <li>"]);
        let markdown = "# One\n\ntext\n\n## Two\n\nmore\n";
        assert_eq!(folds("markdown", markdown), ["0..6 # One", "4..6 ## Two"]);
    }

    #[test]
    fn plain_text_folds_by_indentation() {
        let source = "a\n  b\n\n    c\n  d\ne\n\n  f\n";
        let text = Rope::from_str(source);
        assert_eq!(
            describe(&text, source, indent_fold_ranges(&text, 4)),
            ["0..4 a", "1..3   b", "5..7 e"]
        );
    }
}
