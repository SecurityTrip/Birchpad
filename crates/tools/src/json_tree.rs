//! JSON as a tree, as JSON Viewer's panel shows it: each value with its key or index and its
//! place in the text; the path of a value (`$.store.book[0]["a b"]`) and the value a path
//! names; and the text again with every object's keys sorted.
//!
//! The tree is a flat list in document order, each node knowing its parent and children, so
//! that neither building it nor dropping it recurses however deep the text nests.

use std::fmt::Write as _;
use std::ops::Range;

use birchpad_i18n::t;

use crate::json::{Kind, check, lex};
use crate::{Layout, SyntaxError};

/// The most values a tree holds; a larger text is refused rather than shown.
pub const MAX_NODES: usize = 1_000_000;

/// What a value is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonKind {
    Object,
    Array,
    String,
    Number,
    Bool,
    Null,
}

/// How a value is reached from its parent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonKey {
    /// A member of an object, by its name (unescaped).
    Name(String),
    /// An element of an array, from 0.
    Index(usize),
}

/// A value of the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonNode {
    /// `None` for the root.
    pub key: Option<JsonKey>,
    /// The key as written, quotes and escapes included, for a member of an object.
    pub key_range: Option<Range<usize>>,
    pub kind: JsonKind,
    /// The value in the text: from its first to its last character.
    pub range: Range<usize>,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    /// 0 for the root.
    pub depth: usize,
}

/// The values of a JSON text, the root first, in the order of the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonTree {
    pub nodes: Vec<JsonNode>,
}

/// A path that does not read, or names no value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    #[error("{}", t!("tools.json.path-does-not-read", reason = .0))]
    Syntax(String),
    #[error("{}", t!("tools.json.path-not-found", path = .0))]
    NotFound(String),
}

/// The text of a string token (with its quotes) as the string it stands for.
pub fn unescape(raw: &str) -> String {
    let inner = raw
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(raw);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('b') => out.push('\u{8}'),
            Some('f') => out.push('\u{c}'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                let unit = u16::from_str_radix(&hex, 16).unwrap_or(0xfffd);
                // A surrogate pair is two escapes.
                if (0xd800..0xdc00).contains(&unit) {
                    let rest = chars.as_str();
                    if let Some(low) = rest
                        .strip_prefix("\\u")
                        .and_then(|low| low.get(..4))
                        .and_then(|low| u16::from_str_radix(low, 16).ok())
                        .filter(|low| (0xdc00..0xe000).contains(low))
                    {
                        let code = 0x10000
                            + ((u32::from(unit) - 0xd800) << 10)
                            + (u32::from(low) - 0xdc00);
                        out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        chars = rest[6..].chars();
                        continue;
                    }
                }
                out.push(char::from_u32(u32::from(unit)).unwrap_or('\u{fffd}'));
            }
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

fn scalar_kind(raw: &str) -> JsonKind {
    match raw.as_bytes().first() {
        Some(b'"') => JsonKind::String,
        Some(b't' | b'f') => JsonKind::Bool,
        Some(b'n') => JsonKind::Null,
        _ => JsonKind::Number,
    }
}

impl JsonTree {
    /// The tree of `text`, which must be one JSON value.
    pub fn parse(text: &str) -> Result<Self, SyntaxError> {
        let tokens = lex(text)?;
        check(text, &tokens)?;
        let mut nodes: Vec<JsonNode> = Vec::new();
        // The open objects and arrays, innermost last.
        let mut open: Vec<usize> = Vec::new();
        // The key read for the next member.
        let mut key: Option<(String, Range<usize>)> = None;
        let mut expect_key = false;
        for token in &tokens {
            let raw = &text[token.start..token.end];
            match token.kind {
                Kind::Colon | Kind::Comma => {
                    // After a comma in an object, a key comes.
                    if token.kind == Kind::Comma {
                        expect_key = open
                            .last()
                            .is_some_and(|&at| nodes[at].kind == JsonKind::Object);
                    }
                    continue;
                }
                Kind::CloseObject | Kind::CloseArray => {
                    if let Some(at) = open.pop() {
                        nodes[at].range.end = token.end;
                    }
                    continue;
                }
                Kind::String if expect_key => {
                    key = Some((unescape(raw), token.start..token.end));
                    expect_key = false;
                    continue;
                }
                _ => {}
            }
            if nodes.len() == MAX_NODES {
                return Err(SyntaxError::new(
                    token.start,
                    t!("tools.json.too-many-values", limit = MAX_NODES),
                ));
            }
            let parent = open.last().copied();
            let (key, key_range) = match parent.map(|at| nodes[at].kind) {
                Some(JsonKind::Object) => match key.take() {
                    Some((name, range)) => (Some(JsonKey::Name(name)), Some(range)),
                    None => (None, None),
                },
                Some(_) => {
                    let siblings = parent.map_or(0, |at| nodes[at].children.len());
                    (Some(JsonKey::Index(siblings)), None)
                }
                None => (None, None),
            };
            let kind = match token.kind {
                Kind::OpenObject => JsonKind::Object,
                Kind::OpenArray => JsonKind::Array,
                _ => scalar_kind(raw),
            };
            let at = nodes.len();
            nodes.push(JsonNode {
                key,
                key_range,
                kind,
                range: token.start..token.end,
                parent,
                children: Vec::new(),
                depth: open.len(),
            });
            if let Some(parent) = parent {
                nodes[parent].children.push(at);
            }
            if matches!(kind, JsonKind::Object | JsonKind::Array) {
                open.push(at);
                // `{` is followed by a key (or `}`).
                expect_key = kind == JsonKind::Object;
            }
        }
        Ok(Self { nodes })
    }

    /// The path of node `index`: `$`, then `.name` for a member whose name is a plain word,
    /// `["name"]` for another, `[n]` for an element.
    pub fn path(&self, index: usize) -> String {
        let mut chain = Vec::new();
        let mut at = Some(index);
        while let Some(node) = at.and_then(|at| self.nodes.get(at)) {
            if let Some(key) = &node.key {
                chain.push(key);
            }
            at = node.parent;
        }
        let mut path = String::from("$");
        for key in chain.into_iter().rev() {
            match key {
                JsonKey::Index(index) => {
                    let _ = write!(path, "[{index}]");
                }
                JsonKey::Name(name) if is_plain(name) => {
                    let _ = write!(path, ".{name}");
                }
                JsonKey::Name(name) => {
                    let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
                    let _ = write!(path, "[\"{escaped}\"]");
                }
            }
        }
        path
    }

    /// The node `path` names: `$` or nothing for the root, then `.name`, `["name"]` (or
    /// `['name']`) and `[n]`; a path may start with a name without the dot (`store.book`).
    pub fn find(&self, path: &str) -> Result<usize, PathError> {
        let steps = parse_path(path)?;
        let mut at = 0;
        for step in &steps {
            let node = &self.nodes[at];
            at = node
                .children
                .iter()
                .copied()
                .find(|&child| self.nodes[child].key.as_ref() == Some(step))
                .ok_or_else(|| PathError::NotFound(path.trim().to_owned()))?;
        }
        Ok(at)
    }

    /// The value of node `index` to copy: a string without its quotes and escapes, anything
    /// else as written.
    pub fn value<'a>(&self, index: usize, text: &'a str) -> std::borrow::Cow<'a, str> {
        let node = &self.nodes[index];
        let raw = &text[node.range.clone()];
        if node.kind == JsonKind::String {
            std::borrow::Cow::Owned(unescape(raw))
        } else {
            std::borrow::Cow::Borrowed(raw)
        }
    }

    /// The text again, formatted as [`crate::json::format`] does, with the members of every
    /// object in the order of their names (then of the text, for names that are the same).
    pub fn sorted(&self, text: &str, layout: Layout<'_>) -> String {
        let mut out = String::with_capacity(text.len() * 2);
        // What is left to write: a node to open, or the end of an object or array.
        enum Step {
            /// A value, and whether it is the first of its parent's.
            Value(usize, bool),
            Close(usize),
        }
        let mut stack = vec![Step::Value(0, true)];
        while let Some(step) = stack.pop() {
            match step {
                Step::Value(at, first) => {
                    let node = &self.nodes[at];
                    if node.parent.is_some() {
                        if !first {
                            out.push(',');
                        }
                        out.push_str(layout.line_ending);
                        layout.indent.write(&mut out, node.depth);
                        if let Some(range) = &node.key_range {
                            out.push_str(&text[range.clone()]);
                            out.push_str(": ");
                        }
                    }
                    match node.kind {
                        JsonKind::Object | JsonKind::Array => {
                            out.push(if node.kind == JsonKind::Object {
                                '{'
                            } else {
                                '['
                            });
                            stack.push(Step::Close(at));
                            let children = self.ordered(at);
                            for (index, child) in children.into_iter().enumerate().rev() {
                                stack.push(Step::Value(child, index == 0));
                            }
                        }
                        _ => out.push_str(&text[node.range.clone()]),
                    }
                }
                Step::Close(at) => {
                    let node = &self.nodes[at];
                    if !node.children.is_empty() {
                        out.push_str(layout.line_ending);
                        layout.indent.write(&mut out, node.depth);
                    }
                    out.push(if node.kind == JsonKind::Object {
                        '}'
                    } else {
                        ']'
                    });
                }
            }
        }
        out
    }

    /// The children of `at` in the order [`Self::sorted`] writes them.
    fn ordered(&self, at: usize) -> Vec<usize> {
        let node = &self.nodes[at];
        let mut children = node.children.clone();
        if node.kind == JsonKind::Object {
            children.sort_by(|a, b| {
                let name = |at: usize| match &self.nodes[at].key {
                    Some(JsonKey::Name(name)) => name.as_str(),
                    _ => "",
                };
                name(*a).cmp(name(*b))
            });
        }
        children
    }
}

/// `text` formatted with the keys of every object sorted.
pub fn sort_keys(text: &str, layout: Layout<'_>) -> Result<String, SyntaxError> {
    Ok(JsonTree::parse(text)?.sorted(text, layout))
}

/// Whether `name` can follow a dot in a path: a letter, `_` or `$`, then also digits.
fn is_plain(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

/// The steps of a path.
fn parse_path(path: &str) -> Result<Vec<JsonKey>, PathError> {
    let path = path.trim();
    let mut rest = path.strip_prefix('$').unwrap_or(path);
    let mut steps = Vec::new();
    let mut first = true;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('[') {
            let close = |quote: char, body: &str| -> Result<(String, usize), PathError> {
                let mut name = String::new();
                let mut chars = body.char_indices();
                while let Some((at, c)) = chars.next() {
                    match c {
                        '\\' => match chars.next() {
                            Some((_, escaped)) => name.push(escaped),
                            None => break,
                        },
                        c if c == quote => {
                            let after = &body[at + 1..];
                            return if after.starts_with(']') {
                                Ok((name, at + 2))
                            } else {
                                Err(PathError::Syntax(t!("tools.json.path-expected-bracket")))
                            };
                        }
                        c => name.push(c),
                    }
                }
                Err(PathError::Syntax(t!("tools.json.path-quote-not-closed")))
            };
            if let Some(body) = after.strip_prefix('"') {
                let (name, used) = close('"', body)?;
                steps.push(JsonKey::Name(name));
                rest = &body[used..];
            } else if let Some(body) = after.strip_prefix('\'') {
                let (name, used) = close('\'', body)?;
                steps.push(JsonKey::Name(name));
                rest = &body[used..];
            } else {
                let end = after
                    .find(']')
                    .ok_or_else(|| PathError::Syntax(t!("tools.json.path-expected-bracket")))?;
                let index = after[..end].trim().parse::<usize>().map_err(|_| {
                    PathError::Syntax(t!(
                        "tools.json.path-not-an-index",
                        text = format!("{:?}", &after[..end])
                    ))
                })?;
                steps.push(JsonKey::Index(index));
                rest = &after[end + 1..];
            }
        } else {
            let name_start = match rest.strip_prefix('.') {
                Some(after) => after,
                None if first => rest,
                None => {
                    return Err(PathError::Syntax(t!(
                        "tools.json.path-unexpected",
                        text = format!("{:?}", rest.chars().next().unwrap_or(' '))
                    )));
                }
            };
            let end = name_start.find(['.', '[']).unwrap_or(name_start.len());
            let name = &name_start[..end];
            if name.is_empty() {
                return Err(PathError::Syntax(t!("tools.json.path-empty-name")));
            }
            steps.push(JsonKey::Name(name.to_owned()));
            rest = &name_start[end..];
        }
        first = false;
    }
    Ok(steps)
}
