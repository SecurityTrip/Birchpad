//! A document's syntax tree: kept roughly right through every edit, reparsed incrementally in
//! the background, and queried for the highlights of the visible text.

use std::cmp::Reverse;
use std::ops::{ControlFlow, Range};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use birchpad_core::{ChangeSet, LineType, Operation, Rope};
use tree_sitter::{
    InputEdit, Node, ParseOptions, Parser, Point, Query, QueryCursor, QueryCursorOptions,
    StreamingIterator, TextProvider, Tree,
};

use crate::highlight::{self, Highlight};
use crate::language::{LANGUAGES, Language};

/// Longest a highlight query may take; the view shows what was found by then.
const HIGHLIGHT_BUDGET: Duration = Duration::from_millis(8);

/// A language's grammar and compiled queries. Compiled once, on first use, and shared by every
/// document in that language.
pub struct LanguageConfig {
    pub language: &'static Language,
    grammar: tree_sitter::Language,
    highlights: Query,
    /// The highlight of each capture of `highlights` (`None` for captures themes ignore).
    capture_highlights: Vec<Option<Highlight>>,
}

impl std::fmt::Debug for LanguageConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LanguageConfig")
            .field("language", &self.language.id)
            .finish_non_exhaustive()
    }
}

type ConfigResult = Result<Arc<LanguageConfig>, String>;

/// The compiled configuration of `language`. Compiling a large query takes a few milliseconds,
/// so it happens once per language per run.
pub fn config(language: &'static Language) -> ConfigResult {
    static CONFIGS: OnceLock<Vec<OnceLock<ConfigResult>>> = OnceLock::new();
    let configs = CONFIGS.get_or_init(|| LANGUAGES.iter().map(|_| OnceLock::new()).collect());
    let index = LANGUAGES
        .iter()
        .position(|known| known == language)
        .expect("languages come from LANGUAGES");
    configs[index].get_or_init(|| compile(language)).clone()
}

fn compile(language: &'static Language) -> ConfigResult {
    let grammar = (language.grammar)();
    let source = language.highlights.join("\n");
    let highlights = Query::new(&grammar, &source)
        .map_err(|error| format!("highlight query of {}: {error}", language.name))?;
    let capture_highlights = highlights
        .capture_names()
        .iter()
        .map(|name| highlight::resolve(name))
        .collect();
    Ok(Arc::new(LanguageConfig {
        language,
        grammar,
        highlights,
        capture_highlights,
    }))
}

/// The syntax tree of one document.
#[derive(Debug)]
pub struct Syntax {
    config: Arc<LanguageConfig>,
    tree: Option<Tree>,
    /// Bumped whenever the tree changes, so views can cache what they computed from it.
    version: u64,
}

impl Syntax {
    /// Starts without a tree; [`Self::parse_job`] produces the first one.
    pub fn new(config: Arc<LanguageConfig>) -> Self {
        Self {
            config,
            tree: None,
            version: 0,
        }
    }

    pub fn language(&self) -> &'static Language {
        self.config.language
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn tree(&self) -> Option<&Tree> {
        self.tree.as_ref()
    }

    /// Follows an edit of `old_text` so that the tree stays roughly right (nodes after the edit
    /// shift with the text) until the reparse finishes.
    pub fn edit(&mut self, old_text: &Rope, changes: &ChangeSet) {
        let Some(tree) = &mut self.tree else {
            return;
        };
        for edit in input_edits(old_text, changes).iter().rev() {
            tree.edit(edit);
        }
        self.version += 1;
    }

    /// What a background parse of `text` needs: it is `Send` and does not borrow the syntax.
    pub fn parse_job(&self, text: Rope) -> ParseJob {
        ParseJob {
            config: self.config.clone(),
            old_tree: self.tree.clone(),
            text,
        }
    }

    /// Takes a tree from a finished [`ParseJob`].
    pub fn install(&mut self, tree: Tree) {
        self.tree = Some(tree);
        self.version += 1;
    }

    /// Highlights of `range`, as non-overlapping sorted spans. Nested captures win over the
    /// nodes around them, and for the same node a later pattern of the query wins over an
    /// earlier one, as the grammars' queries expect (general patterns first, specific ones
    /// after).
    pub fn highlights(&self, text: &Rope, range: Range<usize>) -> Vec<(Range<usize>, Highlight)> {
        let Some(tree) = &self.tree else {
            return Vec::new();
        };
        let range = range.start.min(text.len())..range.end.min(text.len());
        if range.is_empty() {
            return Vec::new();
        }
        // Queries are fast (well under a millisecond for a screen) except on trees that error
        // recovery made very deep, such as a whole file turned into one nested expression
        // after a typo. Those get a time budget and partial highlights instead of a stall.
        let started = Instant::now();
        let mut over_budget = |_: &tree_sitter::QueryCursorState| {
            if started.elapsed() > HIGHLIGHT_BUDGET {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        };
        let options = QueryCursorOptions::new().progress_callback(&mut over_budget);
        let mut cursor = QueryCursor::new();
        cursor.set_byte_range(range.clone());
        let mut found = Vec::new();
        let mut captures = cursor.captures_with_options(
            &self.config.highlights,
            tree.root_node(),
            RopeText(text),
            options,
        );
        while let Some((matched, index)) = captures.next() {
            let capture = matched.captures()[*index];
            if let Some(highlight) = self.config.capture_highlights[capture.index as usize] {
                let node = capture.node.byte_range();
                let start = node.start.max(range.start);
                let end = node.end.min(range.end);
                if start < end {
                    found.push((start, end, matched.pattern_index, highlight));
                }
            }
        }
        found.sort_by_key(|&(start, end, pattern, _)| (start, Reverse(end), pattern));

        // Paint captures in that order over the bytes of the range; later paint wins.
        let mut painted: Vec<Option<Highlight>> = vec![None; range.len()];
        for (start, end, _, highlight) in found {
            painted[start - range.start..end - range.start].fill(Some(highlight));
        }
        let mut spans: Vec<(Range<usize>, Highlight)> = Vec::new();
        let mut index = 0;
        while index < painted.len() {
            let current = painted[index];
            let run = painted[index..]
                .iter()
                .position(|&h| h != current)
                .unwrap_or(painted.len() - index);
            if let Some(highlight) = current {
                let start = range.start + index;
                spans.push((start..start + run, highlight));
            }
            index += run;
        }
        spans
    }
}

/// A parse to run on a background thread.
pub struct ParseJob {
    config: Arc<LanguageConfig>,
    old_tree: Option<Tree>,
    text: Rope,
}

impl ParseJob {
    /// The text being parsed.
    pub fn text(&self) -> &Rope {
        &self.text
    }

    /// Parses the text, reusing the old tree where the text did not change. Returns `None` if
    /// `cancel` was set meanwhile.
    pub fn run(self, cancel: &AtomicBool) -> Option<Tree> {
        let mut parser = Parser::new();
        parser
            .set_language(&self.config.grammar)
            .expect("grammars are built against a compatible tree-sitter");
        let text = &self.text;
        let mut read = |byte: usize, _: Point| -> &[u8] {
            if byte >= text.len() {
                return &[];
            }
            let (chunk, start) = text.chunk(byte);
            &chunk.as_bytes()[byte - start..]
        };
        let mut progress = |_: &tree_sitter::ParseState| {
            if cancel.load(Ordering::Relaxed) {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        };
        let options = ParseOptions::new().progress_callback(&mut progress);
        parser.parse_with_options(&mut read, self.old_tree.as_ref(), Some(options))
    }
}

/// Lets queries read node text (for `#eq?` and `#match?` predicates) straight from the rope.
struct RopeText<'a>(&'a Rope);

impl<'a> TextProvider<&'a [u8]> for RopeText<'a> {
    type I = std::iter::Map<ropey::iter::Chunks<'a>, fn(&'a str) -> &'a [u8]>;

    fn text(&mut self, node: Node) -> Self::I {
        self.0.slice(node.byte_range()).chunks().map(str::as_bytes)
    }
}

/// The edits of `changes` as tree-sitter wants them, in old-text coordinates, in order.
fn input_edits(old_text: &Rope, changes: &ChangeSet) -> Vec<InputEdit> {
    let mut edits = Vec::new();
    let mut old = 0;
    let mut pending: Option<(usize, usize, String)> = None;
    let flush = |pending: &mut Option<(usize, usize, String)>, edits: &mut Vec<InputEdit>| {
        if let Some((start, end, inserted)) = pending.take() {
            let start_position = point_at(old_text, start);
            edits.push(InputEdit {
                start_byte: start,
                old_end_byte: end,
                new_end_byte: start + inserted.len(),
                start_position,
                old_end_position: point_at(old_text, end),
                new_end_position: advance(start_position, &inserted),
            });
        }
    };
    for op in changes.ops() {
        match op {
            Operation::Retain(n) => {
                flush(&mut pending, &mut edits);
                old += n;
            }
            Operation::Delete(n) => {
                pending.get_or_insert_with(|| (old, old, String::new())).1 = old + n;
                old += n;
            }
            Operation::Insert(s) => {
                pending
                    .get_or_insert_with(|| (old, old, String::new()))
                    .2
                    .push_str(s);
            }
        }
    }
    flush(&mut pending, &mut edits);
    edits
}

/// Tree-sitter's point for a byte: rows count `\n` only, columns are bytes.
fn point_at(text: &Rope, byte: usize) -> Point {
    let row = text.byte_to_line_idx(byte, LineType::LF);
    let row_start = text.line_to_byte_idx(row, LineType::LF);
    Point::new(row, byte - row_start)
}

fn advance(point: Point, inserted: &str) -> Point {
    match inserted.rfind('\n') {
        Some(last) => Point::new(
            point.row + inserted.bytes().filter(|&b| b == b'\n').count(),
            inserted.len() - last - 1,
        ),
        None => Point::new(point.row, point.column + inserted.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::by_id;
    use birchpad_core::Edit;

    fn parse(language: &str, text: &Rope) -> Syntax {
        let config = config(by_id(language).unwrap()).unwrap();
        let mut syntax = Syntax::new(config);
        let tree = syntax
            .parse_job(text.clone())
            .run(&AtomicBool::new(false))
            .unwrap();
        syntax.install(tree);
        syntax
    }

    fn named(spans: &[(Range<usize>, Highlight)], text: &str) -> Vec<(String, &'static str)> {
        spans
            .iter()
            .map(|(range, highlight)| (text[range.clone()].to_owned(), highlight.name()))
            .collect()
    }

    #[test]
    fn every_query_compiles() {
        for language in LANGUAGES {
            if let Err(error) = config(language) {
                panic!("{error}");
            }
        }
    }

    #[test]
    fn highlights_rust() {
        let source = "fn main() {\n    // hi\n    let x = \"s\";\n}\n";
        let text = Rope::from_str(source);
        let syntax = parse("rust", &text);
        let spans = named(&syntax.highlights(&text, 0..text.len()), source);
        assert!(spans.contains(&("fn".into(), "keyword")), "{spans:?}");
        assert!(spans.contains(&("main".into(), "function")), "{spans:?}");
        assert!(spans.contains(&("// hi".into(), "comment")), "{spans:?}");
        assert!(spans.contains(&("\"s\"".into(), "string")), "{spans:?}");
        // Only the requested range.
        let tail = syntax.highlights(&text, 22..text.len());
        assert!(tail.iter().all(|(range, _)| range.start >= 22));
    }

    #[test]
    fn specific_patterns_win_over_general_ones() {
        // C's query starts with `(identifier) @variable` and later says call targets are
        // functions.
        let source = "int main(void) { return puts(\"x\"); }";
        let text = Rope::from_str(source);
        let syntax = parse("c", &text);
        let spans = named(&syntax.highlights(&text, 0..text.len()), source);
        assert!(spans.contains(&("puts".into(), "function")), "{spans:?}");
    }

    #[test]
    fn incremental_parse_matches_a_fresh_one() {
        let mut text = Rope::from_str("fn a() {}\n\nfn b() { let x = 1; }\n");
        let mut syntax = parse("rust", &text);
        let edits = [
            vec![Edit::insert(9, "\nstruct S { f: u8 }")],
            vec![Edit::replace(3..4, "c"), Edit::delete(20..22)],
            vec![
                Edit::insert(0, "// lead\n"),
                Edit::insert(text.len(), "\n// end"),
            ],
        ];
        for batch in edits {
            let len = text.len();
            let batch: Vec<Edit> = batch
                .into_iter()
                .map(|e| Edit::replace(e.range.start.min(len)..e.range.end.min(len), e.text))
                .collect();
            let changes = ChangeSet::from_edits(&text, batch).unwrap();
            let old = text.clone();
            changes.apply(&mut text);
            syntax.edit(&old, &changes);
            let tree = syntax
                .parse_job(text.clone())
                .run(&AtomicBool::new(false))
                .unwrap();
            syntax.install(tree);
            let fresh = parse("rust", &text);
            assert_eq!(
                syntax.tree().unwrap().root_node().to_sexp(),
                fresh.tree().unwrap().root_node().to_sexp(),
                "{text}"
            );
        }
    }

    #[test]
    fn a_cancelled_parse_returns_nothing() {
        let text = Rope::from_str(&"fn f() { 1 + 2; }\n".repeat(20_000));
        let config = config(by_id("rust").unwrap()).unwrap();
        let syntax = Syntax::new(config);
        assert!(syntax.parse_job(text).run(&AtomicBool::new(true)).is_none());
    }
}
