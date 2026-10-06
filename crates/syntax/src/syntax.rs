//! A document's syntax tree: kept roughly right through every edit, reparsed incrementally in
//! the background, and queried for the highlights of the visible text.
//!
//! Languages embedded in others (scripts and styles in HTML, code blocks in Markdown, HTML in
//! PHP) get trees of their own, layers parsed over the ranges of the document that hold them.
//! Layers are found with the host language's injection query after each parse, and nest
//! (Markdown, its inline syntax, HTML in it, a script in that). A layer whose text did not
//! change keeps its tree; one that did is reparsed incrementally from its old tree.

use std::cmp::Reverse;
use std::collections::HashMap;
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
use crate::language::{self, Language};

/// Longest a highlight query may take; the view shows what was found by then.
const HIGHLIGHT_BUDGET: Duration = Duration::from_millis(8);

/// How deep embedded languages nest (Markdown > inline > HTML > JavaScript > regex).
const MAX_DEPTH: u8 = 5;

/// At most this many layers per document: a huge Markdown file has one per paragraph.
const MAX_LAYERS: usize = 50_000;

/// A language's grammar and compiled queries. Compiled once, on first use, and shared by every
/// document in that language.
pub struct LanguageConfig {
    pub language: &'static Language,
    grammar: tree_sitter::Language,
    highlights: Query,
    /// The highlight of each capture of `highlights` (`None` for captures themes ignore).
    capture_highlights: Vec<Option<Highlight>>,
    injections: Option<Injections>,
}

/// A compiled injection query: where other languages are embedded.
struct Injections {
    query: Query,
    /// `@injection.content`: the embedded text.
    content: u32,
    /// `@injection.language`: a node naming the language, as a code block's info string does.
    language: Option<u32>,
    /// The `#set!` properties of each pattern.
    patterns: Vec<InjectionPattern>,
}

#[derive(Default)]
struct InjectionPattern {
    /// `injection.language`: the language, when the pattern names it.
    language: Option<String>,
    /// `injection.combined`: all the matches are one document (PHP's HTML around its code).
    combined: bool,
    /// `injection.include-children`: the content node's children are part of the text.
    include_children: bool,
}

impl std::fmt::Debug for LanguageConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LanguageConfig")
            .field("language", &self.language.id)
            .finish_non_exhaustive()
    }
}

impl LanguageConfig {
    /// The highlights the language's query can give, one per capture that has one (for
    /// checking that a sample shows each).
    pub fn capture_highlights(&self) -> Vec<(String, Highlight)> {
        self.highlights
            .capture_names()
            .iter()
            .zip(&self.capture_highlights)
            .filter_map(|(name, highlight)| Some(((*name).to_owned(), (*highlight)?)))
            .collect()
    }
}

type ConfigResult = Result<Arc<LanguageConfig>, String>;

/// The compiled configuration of `language`. Compiling a large query takes a few milliseconds,
/// so it happens once per language per run.
pub fn config(language: &'static Language) -> ConfigResult {
    static CONFIGS: OnceLock<Vec<OnceLock<ConfigResult>>> = OnceLock::new();
    let configs = CONFIGS.get_or_init(|| language::all().map(|_| OnceLock::new()).collect());
    let index = language::all()
        .position(|known| known == language)
        .expect("languages come from LANGUAGES or EMBEDDED");
    configs[index].get_or_init(|| compile(language)).clone()
}

fn compile(language: &'static Language) -> ConfigResult {
    let grammar = (language.grammar)();
    let highlights = query(&grammar, language.highlights)
        .map_err(|error| format!("highlight query of {}: {error}", language.name))?;
    let capture_highlights = highlights
        .capture_names()
        .iter()
        .map(|name| highlight::resolve(name))
        .collect();
    let injections = if language.injections.is_empty() {
        None
    } else {
        let query = query(&grammar, language.injections)
            .map_err(|error| format!("injection query of {}: {error}", language.name))?;
        Some(Injections::new(query))
    };
    Ok(Arc::new(LanguageConfig {
        language,
        grammar,
        highlights,
        capture_highlights,
        injections,
    }))
}

/// Compiles query sources written for Neovim as well as for tree-sitter's own tools: Lua
/// patterns (`#lua-match?`) become regular expressions, and patterns with predicates nothing
/// here evaluates (`#has-ancestor?`) are left out rather than matching too much.
fn query(grammar: &tree_sitter::Language, sources: &[&str]) -> Result<Query, String> {
    let source = lua_matches_to_regex(&sources.join("\n"));
    let mut query = Query::new(grammar, &source).map_err(|error| error.to_string())?;
    for pattern in 0..query.pattern_count() {
        if !query.general_predicates(pattern).is_empty() {
            query.disable_pattern(pattern);
        }
    }
    Ok(query)
}

impl Injections {
    fn new(query: Query) -> Self {
        let index = |name: &str| query.capture_index_for_name(name);
        let content = index("injection.content").unwrap_or(u32::MAX);
        let language = index("injection.language");
        let patterns = (0..query.pattern_count())
            .map(|pattern| {
                let mut settings = InjectionPattern::default();
                for property in query.property_settings(pattern) {
                    match property.key.as_ref() {
                        "injection.language" => {
                            settings.language = property.value.as_deref().map(str::to_owned);
                        }
                        "injection.combined" => settings.combined = true,
                        "injection.include-children" => settings.include_children = true,
                        _ => {}
                    }
                }
                settings
            })
            .collect();
        Self {
            query,
            content,
            language,
            patterns,
        }
    }
}

/// The tree of a language embedded in the document, over the ranges that hold it.
#[derive(Debug, Clone)]
pub struct Layer {
    config: Arc<LanguageConfig>,
    /// Sorted, not overlapping.
    ranges: Vec<tree_sitter::Range>,
    tree: Tree,
    /// 1 for a language embedded in the document's, 2 for one embedded in that, ...
    depth: u8,
    /// An edit touched its text since it was parsed.
    dirty: bool,
    /// The last parse kept its tree: nothing in its text changed.
    reused: bool,
}

impl Layer {
    pub fn language(&self) -> &'static Language {
        self.config.language
    }

    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// Whether the last parse kept its tree, nothing in its text having changed.
    pub fn reused(&self) -> bool {
        self.reused
    }

    /// The bytes of the document it covers, from the start of its first range to the end of
    /// its last.
    pub fn extent(&self) -> Range<usize> {
        let first = self.ranges.first().map_or(0, |range| range.start_byte);
        let last = self.ranges.last().map_or(0, |range| range.end_byte);
        first..last
    }
}

/// A finished parse: the document's tree and the layers of its embedded languages.
#[derive(Debug)]
pub struct Parsed {
    tree: Tree,
    layers: Vec<Layer>,
}

impl Parsed {
    pub fn tree(&self) -> &Tree {
        &self.tree
    }
}

/// The syntax tree of one document.
#[derive(Debug)]
pub struct Syntax {
    config: Arc<LanguageConfig>,
    tree: Option<Tree>,
    /// Sorted by the start of their extent.
    layers: Vec<Layer>,
    /// Bumped whenever the tree changes, so views can cache what they computed from it.
    version: u64,
}

impl Syntax {
    /// Starts without a tree; [`Self::parse_job`] produces the first one.
    pub fn new(config: Arc<LanguageConfig>) -> Self {
        Self {
            config,
            tree: None,
            layers: Vec::new(),
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

    /// The layers of the embedded languages, sorted by where they start.
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    /// Follows an edit of `old_text` so that the trees stay roughly right (nodes after the edit
    /// shift with the text) until the reparse finishes.
    pub fn edit(&mut self, old_text: &Rope, changes: &ChangeSet) {
        let Some(tree) = &mut self.tree else {
            return;
        };
        let edits = input_edits(old_text, changes);
        // In old-text coordinates: the last edit first, so the earlier ones stay valid.
        for edit in edits.iter().rev() {
            tree.edit(edit);
            for layer in &mut self.layers {
                // Text before an edit does not move.
                if layer.extent().end < edit.start_byte {
                    continue;
                }
                layer.tree.edit(edit);
                for range in &mut layer.ranges {
                    if edit.start_byte <= range.end_byte && edit.old_end_byte >= range.start_byte {
                        layer.dirty = true;
                    }
                    range.start_byte = shift(range.start_byte, edit);
                    range.end_byte = shift(range.end_byte, edit);
                }
            }
        }
        self.version += 1;
    }

    /// What a background parse of `text` needs: it is `Send` and does not borrow the syntax.
    pub fn parse_job(&self, text: Rope) -> ParseJob {
        ParseJob {
            config: self.config.clone(),
            old_tree: self.tree.clone(),
            old_layers: self.layers.clone(),
            text,
        }
    }

    /// Takes the trees from a finished [`ParseJob`].
    pub fn install(&mut self, parsed: Parsed) {
        self.tree = Some(parsed.tree);
        self.layers = parsed.layers;
        self.version += 1;
    }

    /// Highlights of `range`, as non-overlapping sorted spans. Nested captures win over the
    /// nodes around them, and for the same node a later pattern of the query wins over an
    /// earlier one, as the grammars' queries expect (general patterns first, specific ones
    /// after). An embedded language's highlights win over its host's.
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
        let mut found = Vec::new();
        capture(
            &self.config,
            tree,
            None,
            0,
            text,
            &range,
            started,
            &mut found,
        );
        for layer in &self.layers {
            let extent = layer.extent();
            if extent.start >= range.end {
                break;
            }
            if extent.end > range.start {
                capture(
                    &layer.config,
                    &layer.tree,
                    Some(layer.ranges.as_slice()),
                    layer.depth,
                    text,
                    &range,
                    started,
                    &mut found,
                );
            }
        }
        found.sort_by_key(|capture| {
            (
                capture.depth,
                capture.start,
                Reverse(capture.end),
                capture.pattern,
            )
        });

        // Paint captures in that order over the bytes of the range; later paint wins.
        let mut painted: Vec<Option<Highlight>> = vec![None; range.len()];
        for capture in found {
            painted[capture.start - range.start..capture.end - range.start]
                .fill(Some(capture.highlight));
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

/// A capture to paint, with what decides which paint wins.
struct Capture {
    depth: u8,
    start: usize,
    end: usize,
    pattern: usize,
    highlight: Highlight,
}

/// Collects the highlight captures of `tree` in `range`; for a layer, only those within its
/// ranges (a node of HTML in PHP may span PHP code between its ranges).
#[allow(
    clippy::too_many_arguments,
    reason = "one call per tree, all of it needed"
)]
fn capture(
    config: &LanguageConfig,
    tree: &Tree,
    ranges: Option<&[tree_sitter::Range]>,
    depth: u8,
    text: &Rope,
    range: &Range<usize>,
    started: Instant,
    found: &mut Vec<Capture>,
) {
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
    let mut captures = cursor.captures_with_options(
        &config.highlights,
        tree.root_node(),
        RopeText(text),
        options,
    );
    while let Some((matched, index)) = captures.next() {
        let captured = matched.captures()[*index];
        let Some(highlight) = config.capture_highlights[captured.index as usize] else {
            continue;
        };
        let node = captured.node.byte_range();
        let mut add = |start: usize, end: usize| {
            let start = start.max(range.start);
            let end = end.min(range.end);
            if start < end {
                found.push(Capture {
                    depth,
                    start,
                    end,
                    pattern: matched.pattern_index,
                    highlight,
                });
            }
        };
        match ranges {
            None => add(node.start, node.end),
            Some(ranges) => {
                let first = ranges.partition_point(|range| range.end_byte <= node.start);
                for range in ranges[first..]
                    .iter()
                    .take_while(|range| range.start_byte < node.end)
                {
                    add(
                        node.start.max(range.start_byte),
                        node.end.min(range.end_byte),
                    );
                }
            }
        }
    }
}

/// A parse to run on a background thread.
pub struct ParseJob {
    config: Arc<LanguageConfig>,
    old_tree: Option<Tree>,
    old_layers: Vec<Layer>,
    text: Rope,
}

impl ParseJob {
    /// The text being parsed.
    pub fn text(&self) -> &Rope {
        &self.text
    }

    /// Parses the text and the languages embedded in it, reusing the old trees where the text
    /// did not change. Returns `None` if `cancel` was set meanwhile.
    pub fn run(self, cancel: &AtomicBool) -> Option<Parsed> {
        let mut parser = Parser::new();
        let tree = parse(
            &mut parser,
            &self.config,
            &self.text,
            None,
            self.old_tree.as_ref(),
            cancel,
        )?;
        let layers = parse_layers(
            &mut parser,
            &self.config,
            &tree,
            &self.text,
            self.old_layers,
            cancel,
        )?;
        Some(Parsed { tree, layers })
    }
}

/// Parses `text` (or only `ranges` of it) in `config`'s language. `None` if cancelled.
fn parse(
    parser: &mut Parser,
    config: &LanguageConfig,
    text: &Rope,
    ranges: Option<&[tree_sitter::Range]>,
    old_tree: Option<&Tree>,
    cancel: &AtomicBool,
) -> Option<Tree> {
    parser
        .set_language(&config.grammar)
        .expect("grammars are built against a compatible tree-sitter");
    parser.set_included_ranges(ranges.unwrap_or(&[])).ok()?;
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
    parser.parse_with_options(&mut read, old_tree, Some(options))
}

/// A language embedded at some ranges, found by an injection query.
struct Injection {
    config: Arc<LanguageConfig>,
    ranges: Vec<tree_sitter::Range>,
}

/// Finds and parses the embedded languages of `tree`, and theirs in turn. Old layers whose
/// language and ranges are the same and whose text did not change keep their trees; others
/// with the same language and start are reparsed from their old trees.
fn parse_layers(
    parser: &mut Parser,
    config: &Arc<LanguageConfig>,
    tree: &Tree,
    text: &Rope,
    old_layers: Vec<Layer>,
    cancel: &AtomicBool,
) -> Option<Vec<Layer>> {
    let mut old = OldLayers::new(old_layers);
    let mut layers: Vec<Layer> = Vec::new();
    let mut hosts = vec![(config.clone(), tree.clone(), 0)];
    while let Some((host, host_tree, depth)) = hosts.pop() {
        if depth >= MAX_DEPTH {
            continue;
        }
        for injection in injections(&host, &host_tree, text) {
            if layers.len() >= MAX_LAYERS || cancel.load(Ordering::Relaxed) {
                break;
            }
            let previous = old.take(injection.config.language.id, &injection.ranges);
            let reused = previous
                .as_ref()
                .is_some_and(|layer| !layer.dirty && same_ranges(&layer.ranges, &injection.ranges));
            let tree = match previous {
                Some(layer) if reused => {
                    // Nothing in its text changed, so neither did the languages embedded in
                    // it: they stay as they are, without running its injection query.
                    for mut nested in old.take_nested(&layer) {
                        nested.reused = true;
                        layers.push(nested);
                    }
                    layer.tree
                }
                previous => {
                    let tree = parse(
                        parser,
                        &injection.config,
                        text,
                        Some(&injection.ranges),
                        previous.as_ref().map(|layer| &layer.tree),
                        cancel,
                    )?;
                    hosts.push((injection.config.clone(), tree.clone(), depth + 1));
                    tree
                }
            };
            layers.push(Layer {
                config: injection.config,
                ranges: injection.ranges,
                tree,
                depth: depth + 1,
                dirty: false,
                reused,
            });
        }
    }
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    layers.sort_by_key(|layer| (layer.extent().start, layer.depth));
    Some(layers)
}

/// The layers of the previous parse, shifted by the edits since, to take over.
struct OldLayers {
    /// Sorted by the start of their extent, as layers are kept; `None` once taken.
    layers: Vec<Option<Layer>>,
    starts: Vec<usize>,
    /// By language and first byte.
    index: HashMap<(&'static str, usize), usize>,
}

impl OldLayers {
    fn new(layers: Vec<Layer>) -> Self {
        let layers: Vec<Layer> = layers
            .into_iter()
            .filter(|layer| !layer.ranges.is_empty())
            .collect();
        let starts = layers.iter().map(|layer| layer.extent().start).collect();
        let index = layers
            .iter()
            .enumerate()
            .map(|(i, layer)| ((layer.config.language.id, layer.ranges[0].start_byte), i))
            .collect();
        Self {
            layers: layers.into_iter().map(Some).collect(),
            starts,
            index,
        }
    }

    /// The old layer of `language` that started where `ranges` start.
    fn take(&mut self, language: &'static str, ranges: &[tree_sitter::Range]) -> Option<Layer> {
        let i = *self.index.get(&(language, ranges.first()?.start_byte))?;
        self.layers[i].take()
    }

    /// The old layers embedded in `host`, at any depth: those deeper whose ranges lie within
    /// its ranges.
    fn take_nested(&mut self, host: &Layer) -> Vec<Layer> {
        let extent = host.extent();
        let first = self.starts.partition_point(|&start| start < extent.start);
        let mut nested = Vec::new();
        for i in first..self.layers.len() {
            if self.starts[i] >= extent.end {
                break;
            }
            let inside = self.layers[i].as_ref().is_some_and(|layer| {
                layer.depth > host.depth
                    && layer.ranges.iter().all(|range| {
                        host.ranges.iter().any(|outer| {
                            outer.start_byte <= range.start_byte && range.end_byte <= outer.end_byte
                        })
                    })
            });
            if inside && let Some(layer) = self.layers[i].take() {
                nested.push(layer);
            }
        }
        nested
    }
}

fn same_ranges(a: &[tree_sitter::Range], b: &[tree_sitter::Range]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(a, b)| a.start_byte == b.start_byte && a.end_byte == b.end_byte)
}

/// The languages embedded in `tree`, by `config`'s injection query. When two patterns embed a
/// language in the same text (JavaScript in any `<script>`, TypeScript in `lang="ts"` ones),
/// the later pattern wins. Languages Birchpad has no grammar for are left out.
fn injections(config: &LanguageConfig, tree: &Tree, text: &Rope) -> Vec<Injection> {
    let Some(injections) = &config.injections else {
        return Vec::new();
    };
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&injections.query, tree.root_node(), RopeText(text));
    // Keyed by the content's first byte; the value keeps the pattern that won.
    let mut separate: HashMap<usize, (usize, Injection)> = HashMap::new();
    let mut combined: HashMap<(usize, &'static str), Injection> = HashMap::new();
    while let Some(matched) = matches.next() {
        let settings = &injections.patterns[matched.pattern_index];
        let name = settings.language.clone().or_else(|| {
            let node = matched
                .captures()
                .iter()
                .find(|capture| Some(capture.index) == injections.language)?
                .node;
            Some(text.slice(node.byte_range()).to_string())
        });
        let Some(language) = name.as_deref().and_then(language::injected) else {
            continue;
        };
        let Ok(language_config) = self::config(language) else {
            continue;
        };
        let mut ranges = Vec::new();
        for captured in matched.captures() {
            if captured.index == injections.content {
                content_ranges(captured.node, settings.include_children, &mut ranges);
            }
        }
        if ranges.is_empty() {
            continue;
        }
        if settings.combined {
            combined
                .entry((matched.pattern_index, language.id))
                .or_insert_with(|| Injection {
                    config: language_config,
                    ranges: Vec::new(),
                })
                .ranges
                .extend(ranges);
        } else {
            let start = ranges[0].start_byte;
            let injection = Injection {
                config: language_config,
                ranges,
            };
            match separate.get(&start) {
                Some((pattern, _)) if *pattern > matched.pattern_index => {}
                _ => {
                    separate.insert(start, (matched.pattern_index, injection));
                }
            }
        }
    }
    let mut found: Vec<Injection> = separate.into_values().map(|(_, found)| found).collect();
    for mut injection in combined.into_values() {
        injection.ranges.sort_by_key(|range| range.start_byte);
        injection
            .ranges
            .dedup_by(|later, earlier| later.start_byte < earlier.end_byte);
        found.push(injection);
    }
    found.sort_by_key(|injection| injection.ranges[0].start_byte);
    found
}

/// The text of an injection's content node: all of it, or without its named children (a
/// Markdown paragraph without the `>` of the quote it is in). Anonymous children are part of
/// the text, as in Neovim, whose queries these are: Markdown's block grammar keeps the `*` and
/// backticks of a paragraph as tokens.
fn content_ranges(node: Node, include_children: bool, ranges: &mut Vec<tree_sitter::Range>) {
    if include_children {
        ranges.push(node.range());
        return;
    }
    let mut start = (node.start_byte(), node.start_position());
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.start_byte() > start.0 {
            ranges.push(tree_sitter::Range {
                start_byte: start.0,
                end_byte: child.start_byte(),
                start_point: start.1,
                end_point: child.start_position(),
            });
        }
        start = (child.end_byte(), child.end_position());
    }
    if node.end_byte() > start.0 {
        ranges.push(tree_sitter::Range {
            start_byte: start.0,
            end_byte: node.end_byte(),
            start_point: start.1,
            end_point: node.end_position(),
        });
    }
}

/// Where `byte` of the old text is after `edit`: a byte inside the replaced text goes to the
/// end of the new text.
fn shift(byte: usize, edit: &InputEdit) -> usize {
    if byte <= edit.start_byte {
        byte
    } else if byte >= edit.old_end_byte {
        byte - edit.old_end_byte + edit.new_end_byte
    } else {
        edit.new_end_byte
    }
}

/// Neovim queries match with Lua patterns (`#lua-match? @x "^[%u_]+$"`); tree-sitter has
/// regular expressions (`#match?`). Rewrites the first into the second.
pub(crate) fn lua_matches_to_regex(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(at) = rest.find("lua-match?") {
        out.push_str(&rest[..at]);
        out.push_str("match?");
        rest = &rest[at + "lua-match?".len()..];
        // The pattern is the next string literal, after the capture.
        let Some(open) = rest.find('"') else {
            break;
        };
        out.push_str(&rest[..=open]);
        rest = &rest[open + 1..];
        let mut close = 0;
        let bytes = rest.as_bytes();
        while close < bytes.len() && bytes[close] != b'"' {
            close += if bytes[close] == b'\\' { 2 } else { 1 };
        }
        let close = close.min(rest.len());
        out.push_str(&lua_pattern_to_regex(&rest[..close]));
        rest = &rest[close..];
    }
    out.push_str(rest);
    out
}

/// A Lua pattern as a regular expression, both as written inside a query's string literal
/// (where a regex backslash is written `\\`).
fn lua_pattern_to_regex(pattern: &str) -> String {
    let class = |c: char, in_set: bool| -> Option<&'static str> {
        Some(match (c, in_set) {
            ('a', false) => "[A-Za-z]",
            ('a', true) => "A-Za-z",
            ('d', _) => "\\\\d",
            ('l', false) => "[a-z]",
            ('l', true) => "a-z",
            ('u', false) => "[A-Z]",
            ('u', true) => "A-Z",
            ('w', false) => "[A-Za-z0-9]",
            ('w', true) => "A-Za-z0-9",
            ('x', false) => "[0-9A-Fa-f]",
            ('x', true) => "0-9A-Fa-f",
            ('s', _) => "\\\\s",
            ('p', false) => "[[:punct:]]",
            ('p', true) => "[:punct:]",
            _ => return None,
        })
    };
    let mut out = String::new();
    let mut in_set = false;
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '%' => match chars.next() {
                Some(next) if next.is_ascii_alphabetic() => match class(next, in_set) {
                    Some(regex) => out.push_str(regex),
                    None => out.push(next),
                },
                // An escaped character: `%.`, `%-`, `%%`.
                Some(next) if next.is_ascii_punctuation() && next != '%' => {
                    out.push_str("\\\\");
                    out.push(next);
                }
                Some(next) => out.push(next),
                None => {}
            },
            '[' if !in_set => {
                in_set = true;
                out.push(c);
                if chars.peek() == Some(&'^') {
                    out.push('^');
                    chars.next();
                }
            }
            ']' if in_set => {
                in_set = false;
                out.push(c);
            }
            // Lua's lazy repetition.
            '-' if !in_set => out.push_str("*?"),
            _ => out.push(c),
        }
    }
    out
}

/// Lets queries read node text (for `#eq?` and `#match?` predicates) straight from the rope.
pub(crate) struct RopeText<'a>(pub(crate) &'a Rope);

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
    use crate::language::{LANGUAGES, by_id};
    use birchpad_core::Edit;

    fn parse(language: &str, text: &Rope) -> Syntax {
        let config = config(by_id(language).unwrap()).unwrap();
        let mut syntax = Syntax::new(config);
        let parsed = syntax
            .parse_job(text.clone())
            .run(&AtomicBool::new(false))
            .unwrap();
        syntax.install(parsed);
        syntax
    }

    fn named(spans: &[(Range<usize>, Highlight)], text: &str) -> Vec<(String, &'static str)> {
        spans
            .iter()
            .map(|(range, highlight)| (text[range.clone()].to_owned(), highlight.name()))
            .collect()
    }

    /// Applies `edits` to the text and the syntax, and parses again.
    fn edit_and_parse(syntax: &mut Syntax, text: &mut Rope, edits: Vec<Edit>) {
        let changes = ChangeSet::from_edits(text, edits).unwrap();
        let old = text.clone();
        changes.apply(text);
        syntax.edit(&old, &changes);
        let parsed = syntax
            .parse_job(text.clone())
            .run(&AtomicBool::new(false))
            .unwrap();
        syntax.install(parsed);
    }

    fn layer_summary(syntax: &Syntax, text: &Rope) -> Vec<(&'static str, String)> {
        syntax
            .layers()
            .iter()
            .map(|layer| {
                let content: String = layer
                    .ranges
                    .iter()
                    .map(|range| text.slice(range.start_byte..range.end_byte).to_string())
                    .collect();
                (layer.language().id, content)
            })
            .collect()
    }

    #[test]
    fn every_query_compiles() {
        for language in language::all() {
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
            edit_and_parse(&mut syntax, &mut text, batch);
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

    #[test]
    fn scripts_and_styles_in_html_are_highlighted() {
        let source = "<p>if</p>\n<script>const x = 1;</script>\n<style>p { color: red; }</style>\n";
        let text = Rope::from_str(source);
        let syntax = parse("html", &text);
        assert_eq!(
            layer_summary(&syntax, &text),
            [
                ("javascript", "const x = 1;".to_owned()),
                ("css", "p { color: red; }".to_owned()),
            ]
        );
        let spans = named(&syntax.highlights(&text, 0..text.len()), source);
        assert!(spans.contains(&("const".into(), "keyword")), "{spans:?}");
        assert!(spans.contains(&("color".into(), "property")), "{spans:?}");
        // "if" in the text is not a keyword: only the script is JavaScript.
        assert!(!spans.contains(&("if".into(), "keyword")), "{spans:?}");
    }

    #[test]
    fn markdown_highlights_code_blocks_and_inline_syntax() {
        let source = "# Title\n\nSome *emphasis* and `code`.\n\n```rust\nfn main() {}\n```\n\n```unknown\nfn x\n```\n";
        let text = Rope::from_str(source);
        let syntax = parse("markdown", &text);
        let languages: Vec<&str> = layer_summary(&syntax, &text)
            .into_iter()
            .map(|(language, _)| language)
            .collect();
        assert!(languages.contains(&"rust"), "{languages:?}");
        assert!(languages.contains(&"markdown_inline"), "{languages:?}");
        let spans = named(&syntax.highlights(&text, 0..text.len()), source);
        assert!(spans.contains(&("fn".into(), "keyword")), "{spans:?}");
        assert!(spans.contains(&("main".into(), "function")), "{spans:?}");
        assert!(
            spans
                .iter()
                .any(|(token, name)| token.contains("emphasis") && *name == "markup.italic"),
            "{spans:?}"
        );
        // A block in a language without a grammar stays as Markdown shows code.
        assert!(!spans.contains(&("x".into(), "variable")), "{spans:?}");
    }

    #[test]
    fn php_is_html_around_its_code() {
        let source = "<div class=\"a\"><?php echo $x; ?></div>\n";
        let text = Rope::from_str(source);
        let syntax = parse("php", &text);
        assert_eq!(layer_summary(&syntax, &text)[0].0, "html");
        let spans = named(&syntax.highlights(&text, 0..text.len()), source);
        assert!(spans.contains(&("div".into(), "tag")), "{spans:?}");
        assert!(spans.contains(&("echo".into(), "keyword")), "{spans:?}");
    }

    #[test]
    fn layers_follow_edits_and_keep_unchanged_trees() {
        let mut text =
            Rope::from_str("```js\nlet a = 1;\n```\n\ntext\n\n```python\ndef f(): pass\n```\n");
        let mut syntax = parse("markdown", &text);
        let python = |syntax: &Syntax| {
            syntax
                .layers()
                .iter()
                .find(|layer| layer.language().id == "python")
                .map(|layer| (layer.extent(), layer.reused))
                .unwrap()
        };
        let (extent, _) = python(&syntax);

        // Typing in the JavaScript block shifts the Python block, whose tree stays.
        edit_and_parse(&mut syntax, &mut text, vec![Edit::insert(11, "bc")]);
        let (moved, reused) = python(&syntax);
        assert_eq!(moved, extent.start + 2..extent.end + 2);
        assert!(reused, "an unchanged layer keeps its tree");
        let source = text.to_string();
        let spans = named(&syntax.highlights(&text, 0..text.len()), &source);
        assert!(spans.contains(&("abc".into(), "variable")), "{spans:?}");
        assert!(spans.contains(&("def".into(), "keyword")), "{spans:?}");

        // Editing the Python block reparses it.
        let at = source.find("pass").unwrap();
        edit_and_parse(
            &mut syntax,
            &mut text,
            vec![Edit::replace(at..at + 4, "return 1")],
        );
        assert!(!python(&syntax).1, "a changed layer is parsed again");
        let source = text.to_string();
        let spans = named(&syntax.highlights(&text, 0..text.len()), &source);
        assert!(spans.contains(&("return".into(), "keyword")), "{spans:?}");

        // The same as parsing the new text afresh.
        let fresh = parse("markdown", &text);
        assert_eq!(layer_summary(&syntax, &text), layer_summary(&fresh, &text));
    }

    #[test]
    fn typescript_scripts_in_svelte_win_over_javascript() {
        let source = "<script lang=\"ts\">let n: number = 1;</script>\n<p>{n + 1}</p>\n";
        let text = Rope::from_str(source);
        let syntax = parse("svelte", &text);
        let languages: Vec<&str> = layer_summary(&syntax, &text)
            .into_iter()
            .map(|(language, _)| language)
            .collect();
        assert_eq!(languages, ["typescript", "javascript"]);
    }

    #[test]
    fn lua_patterns_become_regular_expressions() {
        assert_eq!(
            lua_pattern_to_regex("^[%u@][%u%d_]+$"),
            "^[A-Z@][A-Z\\\\d_]+$"
        );
        assert_eq!(lua_pattern_to_regex("^%a%w*$"), "^[A-Za-z][A-Za-z0-9]*$");
        assert_eq!(lua_pattern_to_regex("^%-%-.-$"), "^\\\\-\\\\-.*?$");
        assert_eq!(lua_pattern_to_regex("[^%s]"), "[^\\\\s]");
        let query = "((identifier) @constant (#lua-match? @constant \"^[%u_]+$\"))";
        assert_eq!(
            lua_matches_to_regex(query),
            "((identifier) @constant (#match? @constant \"^[A-Z_]+$\"))"
        );
    }

    #[test]
    fn every_lua_class_inside_and_outside_sets() {
        for (lua, regex) in [
            ("%a", "[A-Za-z]"),
            ("[%a]", "[A-Za-z]"),
            ("%d", "\\\\d"),
            ("[%d]", "[\\\\d]"),
            ("%l", "[a-z]"),
            ("[%l]", "[a-z]"),
            ("%u", "[A-Z]"),
            ("[%u]", "[A-Z]"),
            ("%w", "[A-Za-z0-9]"),
            ("[%w]", "[A-Za-z0-9]"),
            ("%x", "[0-9A-Fa-f]"),
            ("[%x]", "[0-9A-Fa-f]"),
            ("%s", "\\\\s"),
            ("%p", "[[:punct:]]"),
            ("[%p]", "[[:punct:]]"),
        ] {
            assert_eq!(lua_pattern_to_regex(lua), regex, "{lua}");
        }
    }

    #[test]
    fn lua_patterns_at_their_edges() {
        // An unknown class is the letter; `%%` a percent sign; a `%` at the very end nothing.
        assert_eq!(lua_pattern_to_regex("%z"), "z");
        assert_eq!(lua_pattern_to_regex("100%%"), "100%");
        assert_eq!(lua_pattern_to_regex("end%"), "end");
        assert_eq!(lua_pattern_to_regex(""), "");
        // A `]` outside a set, and a `^` that does not start a set.
        assert_eq!(lua_pattern_to_regex("a]^b"), "a]^b");
    }

    #[test]
    fn lua_matches_in_queries_at_their_edges() {
        // Two in one query, an escaped quote inside the pattern, none at all.
        let two = r#"(#lua-match? @a "^%u") (#lua-match? @b "%d\"x")"#;
        assert_eq!(
            lua_matches_to_regex(two),
            r#"(#match? @a "^[A-Z]") (#match? @b "\\d\"x")"#
        );
        assert_eq!(lua_matches_to_regex("(#eq? @a \"x\")"), "(#eq? @a \"x\")");
        // A predicate without its string is left as it is (the query then fails to compile).
        assert_eq!(lua_matches_to_regex("(#lua-match? @a"), "(#match? @a");
        // An unterminated string runs to the end.
        assert_eq!(
            lua_matches_to_regex("(#lua-match? @a \"%d"),
            "(#match? @a \"\\\\d"
        );
    }

    #[test]
    fn a_syntax_without_a_tree_has_no_highlights() {
        let config = config(by_id("rust").unwrap()).unwrap();
        assert!(format!("{config:?}").contains("rust"));
        let mut syntax = Syntax::new(config);
        let text = Rope::from_str("fn main() {}");
        assert!(syntax.highlights(&text, 0..text.len()).is_empty());
        // Following an edit before the first parse is no problem.
        let changes = ChangeSet::from_edits(&text, [Edit::insert(0, "x")]).unwrap();
        syntax.edit(&text, &changes);
        assert!(syntax.layers().is_empty());
    }

    #[test]
    fn highlights_of_empty_and_overlong_ranges() {
        let text = Rope::from_str("fn main() {}");
        let syntax = parse("rust", &text);
        assert!(syntax.highlights(&text, 3..3).is_empty());
        assert!(syntax.highlights(&text, 50..60).is_empty(), "past the end");
        // A range running past the end is cut there.
        let whole = syntax.highlights(&text, 0..text.len());
        assert_eq!(syntax.highlights(&text, 0..1000), whole);
    }

    #[test]
    fn embedded_languages_stop_at_the_depth_limit() {
        // Markdown in a Markdown code block, eight deep, each fence longer than the inner one.
        let mut source = String::from("deepest *text*\n");
        for level in 0..8 {
            let fence = "`".repeat(3 + level);
            source = format!("{fence}markdown\n{source}{fence}\n");
        }
        let text = Rope::from_str(&source);
        let syntax = parse("markdown", &text);
        let depths: Vec<u8> = syntax.layers().iter().map(|layer| layer.depth).collect();
        assert!(!depths.is_empty());
        assert!(depths.iter().all(|&depth| depth <= MAX_DEPTH), "{depths:?}");
        assert!(
            depths.contains(&MAX_DEPTH),
            "nesting goes down to the limit: {depths:?}"
        );
        // A first parse reuses nothing; highlighting still covers the whole text.
        assert!(syntax.layers().iter().all(|layer| !layer.reused()));
        assert!(!syntax.highlights(&text, 0..text.len()).is_empty());
    }

    #[test]
    fn every_language_highlights_something() {
        // Catches a grammar whose query compiles but matches nothing (wrong node names).
        for language in LANGUAGES {
            let config = config(language).unwrap();
            assert!(
                config.highlights.pattern_count() > 0,
                "{} has no highlight patterns",
                language.id
            );
        }
    }
}
