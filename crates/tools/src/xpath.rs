//! XPath, as XML Tools' "Evaluate XPath expression" uses it: the nodes of a document an
//! expression selects, each with its place in the text, or the value it computes.
//!
//! A part of XPath 1.0: location paths with `/` and `//`, the child, attribute, self and parent
//! steps (`name`, `*`, `@name`, `@*`, `text()`, `node()`, `comment()`, `.`, `..`), unions with
//! `|`, and predicates with positions, comparisons (`=`, `!=`, `<`, `<=`, `>`, `>=`), `and`, `or`,
//! `not()` and the functions `position()`, `last()`, `count()`, `contains()`, `starts-with()`,
//! `string-length()`, `normalize-space()`, `name()` and `local-name()`. Names match by their local
//! part: a prefix is not resolved to a namespace.

use std::ops::Range;

use birchpad_i18n::t;
use roxmltree::{Document, Node, NodeType, ParsingOptions};

/// The most nodes a result holds; more are left out.
pub const MAX_RESULTS: usize = 100_000;
/// How deep parentheses and predicates may nest.
const MAX_NESTING: usize = 64;

/// What an expression gave.
#[derive(Debug, Clone, PartialEq)]
pub enum XPathResult {
    /// Nodes, in the order of the document.
    Nodes(Vec<XPathMatch>),
    /// A number, a string or a truth value, as text.
    Value(String),
}

/// A node an expression selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XPathMatch {
    /// The node in the text.
    pub range: Range<usize>,
    /// How the results list shows it: `<name>`, `@name="value"`, the text, the comment.
    pub label: String,
}

/// An expression that does not read, or a document that is not XML.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct XPathError(pub String);

/// Evaluates `expression` over the XML document `text`.
pub fn evaluate(text: &str, expression: &str) -> Result<XPathResult, XPathError> {
    let expr = Parser::new(expression).parse()?;
    let options = ParsingOptions {
        allow_dtd: true,
        ..ParsingOptions::default()
    };
    let document = Document::parse_with_options(text, options)
        .map_err(|error| XPathError(t!("tools.xpath.not-xml", reason = error)))?;
    let root = Item::Node(document.root());
    let context = Context {
        item: root,
        position: 1,
        size: 1,
    };
    Ok(match eval(&expr, &context)? {
        Value::Nodes(mut items) => {
            sort_items(&mut items);
            items.truncate(MAX_RESULTS);
            XPathResult::Nodes(items.iter().map(|item| item.matched(text)).collect())
        }
        Value::Number(number) => XPathResult::Value(number_text(number)),
        Value::Text(text) => XPathResult::Value(text),
        Value::Bool(truth) => XPathResult::Value(truth.to_string()),
    })
}

fn number_text(number: f64) -> String {
    if number.is_finite() && number.fract() == 0.0 && number.abs() < 1e15 {
        format!("{}", number as i64)
    } else if number.is_nan() {
        "NaN".to_owned()
    } else {
        number.to_string()
    }
}

// --- The expression ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Test {
    /// A name (its local part), or any (`*`).
    Name(Option<String>),
    Text,
    Comment,
    Node,
}

#[derive(Debug, Clone, PartialEq)]
enum Axis {
    Child,
    /// Children of the context and of every node below it (the `//` before a step).
    Descendant,
    Attribute,
    SelfNode,
    Parent,
}

#[derive(Debug, Clone, PartialEq)]
struct Step {
    axis: Axis,
    test: Test,
    predicates: Vec<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
struct Path {
    /// From the document's root rather than the context.
    absolute: bool,
    steps: Vec<Step>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Compare {
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Path(Path),
    Union(Vec<Expr>),
    Number(f64),
    Literal(String),
    Or(Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Compare(Compare, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}

struct Parser<'a> {
    text: &'a str,
    at: usize,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            at: 0,
            depth: 0,
        }
    }

    fn rest(&self) -> &'a str {
        &self.text[self.at..]
    }

    fn skip_space(&mut self) {
        let rest = self.rest();
        self.at += rest.len() - rest.trim_start().len();
    }

    fn eat(&mut self, token: &str) -> bool {
        self.skip_space();
        if self.rest().starts_with(token) {
            self.at += token.len();
            true
        } else {
            false
        }
    }

    /// `word` as a whole word (`and`, `or`), not the start of a name.
    fn eat_word(&mut self, word: &str) -> bool {
        self.skip_space();
        let rest = self.rest();
        let follows = rest[word.len().min(rest.len())..].chars().next();
        if rest.starts_with(word) && !follows.is_some_and(is_name_char) {
            self.at += word.len();
            true
        } else {
            false
        }
    }

    fn unexpected(&self) -> XPathError {
        match self.rest().chars().next() {
            Some(found) => XPathError(t!(
                "tools.xpath.unexpected",
                found = format!("{found:?}"),
                position = self.at + 1
            )),
            None => XPathError(t!("tools.xpath.unexpected-end")),
        }
    }

    fn parse(mut self) -> Result<Expr, XPathError> {
        self.skip_space();
        if self.rest().is_empty() {
            return Err(XPathError(t!("tools.xpath.empty")));
        }
        let expr = self.or()?;
        self.skip_space();
        if !self.rest().is_empty() {
            return Err(self.unexpected());
        }
        Ok(expr)
    }

    fn nested<T>(
        &mut self,
        parse: impl FnOnce(&mut Self) -> Result<T, XPathError>,
    ) -> Result<T, XPathError> {
        if self.depth == MAX_NESTING {
            return Err(XPathError(t!(
                "tools.xpath.nested-too-deep",
                levels = MAX_NESTING
            )));
        }
        self.depth += 1;
        let result = parse(self);
        self.depth -= 1;
        result
    }

    fn or(&mut self) -> Result<Expr, XPathError> {
        let mut left = self.and()?;
        while self.eat_word("or") {
            left = Expr::Or(Box::new(left), Box::new(self.and()?));
        }
        Ok(left)
    }

    fn and(&mut self) -> Result<Expr, XPathError> {
        let mut left = self.comparison()?;
        while self.eat_word("and") {
            left = Expr::And(Box::new(left), Box::new(self.comparison()?));
        }
        Ok(left)
    }

    fn comparison(&mut self) -> Result<Expr, XPathError> {
        let left = self.union()?;
        for (token, compare) in [
            ("!=", Compare::NotEqual),
            ("<=", Compare::LessOrEqual),
            (">=", Compare::GreaterOrEqual),
            ("=", Compare::Equal),
            ("<", Compare::Less),
            (">", Compare::Greater),
        ] {
            if self.eat(token) {
                let right = self.union()?;
                return Ok(Expr::Compare(compare, Box::new(left), Box::new(right)));
            }
        }
        Ok(left)
    }

    fn union(&mut self) -> Result<Expr, XPathError> {
        let first = self.primary()?;
        let mut parts = vec![first];
        while self.eat("|") {
            parts.push(self.primary()?);
        }
        Ok(if parts.len() == 1 {
            parts.pop().expect("one part")
        } else {
            Expr::Union(parts)
        })
    }

    fn primary(&mut self) -> Result<Expr, XPathError> {
        self.skip_space();
        let rest = self.rest();
        if rest.starts_with('(') {
            self.at += 1;
            let inner = self.nested(Self::or)?;
            if !self.eat(")") {
                return Err(self.unexpected());
            }
            return Ok(inner);
        }
        if let Some(quote) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') {
            let body = &rest[1..];
            let end = body
                .find(quote)
                .ok_or_else(|| XPathError(t!("tools.xpath.quote-not-closed")))?;
            self.at += end + 2;
            return Ok(Expr::Literal(body[..end].to_owned()));
        }
        if rest.starts_with(|c: char| c.is_ascii_digit())
            || (rest.starts_with('.') && rest[1..].starts_with(|c: char| c.is_ascii_digit()))
        {
            let len = rest
                .find(|c: char| !(c.is_ascii_digit() || c == '.'))
                .unwrap_or(rest.len());
            let number = rest[..len].parse::<f64>().map_err(|_| self.unexpected())?;
            self.at += len;
            return Ok(Expr::Number(number));
        }
        if rest.starts_with('-') {
            self.at += 1;
            return match self.primary()? {
                Expr::Number(number) => Ok(Expr::Number(-number)),
                _ => Err(self.unexpected()),
            };
        }
        // A function call: a name followed by `(`, but not a node test.
        let name_len = rest.find(|c: char| !is_name_char(c)).unwrap_or(rest.len());
        let name = &rest[..name_len];
        let after = rest[name_len..].trim_start();
        if name_len > 0 && after.starts_with('(') && !matches!(name, "text" | "node" | "comment") {
            self.at += name_len;
            self.eat("(");
            let mut arguments = Vec::new();
            if !self.eat(")") {
                loop {
                    arguments.push(self.nested(Self::or)?);
                    if self.eat(")") {
                        break;
                    }
                    if !self.eat(",") {
                        return Err(self.unexpected());
                    }
                }
            }
            let known = matches!(
                (name, arguments.len()),
                ("position" | "last", 0)
                    | ("count" | "not" | "contains" | "starts-with", _)
                    | (
                        "string-length" | "normalize-space" | "name" | "local-name",
                        0 | 1
                    )
            );
            let arity = match name {
                "count" | "not" => 1,
                "contains" | "starts-with" => 2,
                _ => arguments.len(),
            };
            if !known || arity != arguments.len() {
                return Err(XPathError(t!(
                    "tools.xpath.unknown-function",
                    name = format!("{name}()")
                )));
            }
            return Ok(Expr::Call(name.to_owned(), arguments));
        }
        self.path().map(Expr::Path)
    }

    fn path(&mut self) -> Result<Path, XPathError> {
        self.skip_space();
        let mut steps = Vec::new();
        let mut absolute = false;
        let mut descendant = false;
        if self.eat("//") {
            absolute = true;
            descendant = true;
        } else if self.eat("/") {
            absolute = true;
            self.skip_space();
            // `/` alone is the document's root.
            let rest = self.rest();
            if rest.is_empty() || rest.starts_with([')', ']', '|', ',', '=', '!', '<', '>']) {
                return Ok(Path { absolute, steps });
            }
        }
        loop {
            steps.push(self.step(descendant)?);
            if self.eat("//") {
                descendant = true;
            } else if self.eat("/") {
                descendant = false;
            } else {
                break;
            }
        }
        Ok(Path { absolute, steps })
    }

    fn step(&mut self, descendant: bool) -> Result<Step, XPathError> {
        self.skip_space();
        let rest = self.rest();
        let (axis, test) = if rest.starts_with("..") {
            self.at += 2;
            (Axis::Parent, Test::Node)
        } else if rest.starts_with('.') {
            self.at += 1;
            (Axis::SelfNode, Test::Node)
        } else if rest.starts_with('@') {
            self.at += 1;
            (Axis::Attribute, Test::Name(self.name_test()?))
        } else {
            let axis = if descendant {
                Axis::Descendant
            } else {
                Axis::Child
            };
            let test = if self.eat("text()") {
                Test::Text
            } else if self.eat("node()") {
                Test::Node
            } else if self.eat("comment()") {
                Test::Comment
            } else {
                Test::Name(self.name_test()?)
            };
            (axis, test)
        };
        let axis = match (axis, descendant) {
            // `//.` and `//..` and `//@a`: from every node below too.
            (Axis::SelfNode | Axis::Parent | Axis::Attribute, true) => {
                return Err(XPathError(t!("tools.xpath.descendant-step")));
            }
            (axis, _) => axis,
        };
        let mut predicates = Vec::new();
        while self.eat("[") {
            predicates.push(self.nested(Self::or)?);
            if !self.eat("]") {
                return Err(self.unexpected());
            }
        }
        Ok(Step {
            axis,
            test,
            predicates,
        })
    }

    /// `*`, or a name whose prefix is left out.
    fn name_test(&mut self) -> Result<Option<String>, XPathError> {
        self.skip_space();
        if self.eat("*") {
            return Ok(None);
        }
        let rest = self.rest();
        let len = rest.find(|c: char| !is_name_char(c)).unwrap_or(rest.len());
        if len == 0 || rest.starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '.') {
            return Err(self.unexpected());
        }
        self.at += len;
        let name = &rest[..len];
        // `prefix:*` or `prefix:name`.
        let local = name.rsplit(':').next().unwrap_or(name);
        Ok(if local == "*" || (local.is_empty() && self.eat("*")) {
            None
        } else {
            Some(local.to_owned())
        })
    }
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':' | '\u{b7}')
}

// --- Evaluation -------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
enum Item<'a, 'input> {
    Node(Node<'a, 'input>),
    /// An element's attribute, by its index.
    Attribute(Node<'a, 'input>, usize),
}

impl PartialEq for Item<'_, '_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Node(a), Self::Node(b)) => a.id() == b.id(),
            (Self::Attribute(a, i), Self::Attribute(b, j)) => a.id() == b.id() && i == j,
            _ => false,
        }
    }
}

impl<'a, 'input> Item<'a, 'input> {
    /// Where it is in the document, for its order.
    fn order(&self) -> (usize, usize) {
        match self {
            Self::Node(node) => (node.id().get_usize(), 0),
            Self::Attribute(node, index) => (node.id().get_usize(), index + 1),
        }
    }

    fn string_value(&self) -> String {
        match self {
            Self::Node(node) => match node.node_type() {
                NodeType::Root | NodeType::Element => node
                    .descendants()
                    .filter(Node::is_text)
                    .filter_map(|text| text.text())
                    .collect(),
                _ => node.text().unwrap_or_default().to_owned(),
            },
            Self::Attribute(node, index) => node
                .attributes()
                .nth(*index)
                .map(|attribute| attribute.value().to_owned())
                .unwrap_or_default(),
        }
    }

    fn local_name(&self) -> String {
        match self {
            Self::Node(node) if node.is_element() => node.tag_name().name().to_owned(),
            Self::Node(_) => String::new(),
            Self::Attribute(node, index) => node
                .attributes()
                .nth(*index)
                .map(|attribute| attribute.name().to_owned())
                .unwrap_or_default(),
        }
    }

    fn matched(&self, text: &str) -> XPathMatch {
        let short = |value: &str| {
            let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
            if value.chars().count() > 60 {
                format!("{}…", value.chars().take(60).collect::<String>())
            } else {
                value
            }
        };
        match self {
            Self::Node(node) => {
                let range = node.range();
                let label = match node.node_type() {
                    NodeType::Element => format!("<{}>", node.tag_name().name()),
                    NodeType::Text => short(node.text().unwrap_or_default()),
                    NodeType::Comment => {
                        format!("<!--{}-->", short(node.text().unwrap_or_default()))
                    }
                    NodeType::PI => short(&text[range.clone()]),
                    NodeType::Root => "/".to_owned(),
                };
                XPathMatch { range, label }
            }
            Self::Attribute(node, index) => match node.attributes().nth(*index) {
                Some(attribute) => XPathMatch {
                    range: attribute.range(),
                    label: format!("@{}=\"{}\"", attribute.name(), short(attribute.value())),
                },
                None => XPathMatch {
                    range: node.range(),
                    label: String::new(),
                },
            },
        }
    }
}

fn sort_items(items: &mut Vec<Item<'_, '_>>) {
    items.sort_by_key(Item::order);
    items.dedup();
}

#[derive(Debug, Clone)]
enum Value<'a, 'input> {
    Nodes(Vec<Item<'a, 'input>>),
    Number(f64),
    Text(String),
    Bool(bool),
}

impl Value<'_, '_> {
    fn truth(&self) -> bool {
        match self {
            Value::Nodes(items) => !items.is_empty(),
            Value::Number(number) => *number != 0.0 && !number.is_nan(),
            Value::Text(text) => !text.is_empty(),
            Value::Bool(truth) => *truth,
        }
    }

    fn text(&self) -> String {
        match self {
            Value::Nodes(items) => items.first().map(Item::string_value).unwrap_or_default(),
            Value::Number(number) => number_text(*number),
            Value::Text(text) => text.clone(),
            Value::Bool(truth) => truth.to_string(),
        }
    }

    fn number(&self) -> f64 {
        match self {
            Value::Number(number) => *number,
            Value::Bool(truth) => f64::from(u8::from(*truth)),
            other => other.text().trim().parse().unwrap_or(f64::NAN),
        }
    }
}

struct Context<'a, 'input> {
    item: Item<'a, 'input>,
    position: usize,
    size: usize,
}

fn eval<'a, 'input>(
    expr: &Expr,
    context: &Context<'a, 'input>,
) -> Result<Value<'a, 'input>, XPathError> {
    Ok(match expr {
        Expr::Number(number) => Value::Number(*number),
        Expr::Literal(text) => Value::Text(text.clone()),
        Expr::Path(path) => Value::Nodes(select(path, context)?),
        Expr::Union(parts) => {
            let mut items = Vec::new();
            for part in parts {
                match eval(part, context)? {
                    Value::Nodes(found) => items.extend(found),
                    _ => return Err(XPathError(t!("tools.xpath.union-of-nodes"))),
                }
            }
            sort_items(&mut items);
            Value::Nodes(items)
        }
        Expr::Or(left, right) => {
            Value::Bool(eval(left, context)?.truth() || eval(right, context)?.truth())
        }
        Expr::And(left, right) => {
            Value::Bool(eval(left, context)?.truth() && eval(right, context)?.truth())
        }
        Expr::Compare(compare, left, right) => Value::Bool(compare_values(
            *compare,
            &eval(left, context)?,
            &eval(right, context)?,
        )),
        Expr::Call(name, arguments) => call(name, arguments, context)?,
    })
}

fn compare_values(compare: Compare, left: &Value<'_, '_>, right: &Value<'_, '_>) -> bool {
    let numbers = |a: f64, b: f64| match compare {
        Compare::Equal => a == b,
        Compare::NotEqual => a != b,
        Compare::Less => a < b,
        Compare::LessOrEqual => a <= b,
        Compare::Greater => a > b,
        Compare::GreaterOrEqual => a >= b,
    };
    let texts = |a: &str, b: &str| match compare {
        Compare::Equal => a == b,
        Compare::NotEqual => a != b,
        _ => numbers(
            a.trim().parse().unwrap_or(f64::NAN),
            b.trim().parse().unwrap_or(f64::NAN),
        ),
    };
    // A node set compares as any of its nodes would.
    let side = |value: &Value<'_, '_>| -> Vec<Value<'static, 'static>> {
        match value {
            Value::Nodes(items) => items
                .iter()
                .map(|item| Value::Text(item.string_value()))
                .collect(),
            Value::Number(number) => vec![Value::Number(*number)],
            Value::Text(text) => vec![Value::Text(text.clone())],
            Value::Bool(truth) => vec![Value::Bool(*truth)],
        }
    };
    if let (Value::Bool(_), _) | (_, Value::Bool(_)) = (left, right) {
        return numbers(
            f64::from(u8::from(left.truth())),
            f64::from(u8::from(right.truth())),
        );
    }
    let (lefts, rights) = (side(left), side(right));
    lefts.iter().any(|a| {
        rights.iter().any(|b| match (a, b) {
            (Value::Number(_), _) | (_, Value::Number(_)) => numbers(a.number(), b.number()),
            _ => texts(&a.text(), &b.text()),
        })
    })
}

fn call<'a, 'input>(
    name: &str,
    arguments: &[Expr],
    context: &Context<'a, 'input>,
) -> Result<Value<'a, 'input>, XPathError> {
    let text_of = |at: usize| -> Result<String, XPathError> {
        match arguments.get(at) {
            Some(argument) => Ok(eval(argument, context)?.text()),
            None => Ok(context.item.string_value()),
        }
    };
    Ok(match name {
        "position" => Value::Number(context.position as f64),
        "last" => Value::Number(context.size as f64),
        "count" => match eval(&arguments[0], context)? {
            Value::Nodes(items) => Value::Number(items.len() as f64),
            _ => return Err(XPathError(t!("tools.xpath.count-of-nodes"))),
        },
        "not" => Value::Bool(!eval(&arguments[0], context)?.truth()),
        "contains" => Value::Bool(text_of(0)?.contains(&text_of(1)?)),
        "starts-with" => Value::Bool(text_of(0)?.starts_with(&text_of(1)?)),
        "string-length" => Value::Number(text_of(0)?.chars().count() as f64),
        "normalize-space" => {
            Value::Text(text_of(0)?.split_whitespace().collect::<Vec<_>>().join(" "))
        }
        "name" | "local-name" => match arguments.first() {
            Some(argument) => match eval(argument, context)? {
                Value::Nodes(items) => {
                    Value::Text(items.first().map(Item::local_name).unwrap_or_default())
                }
                _ => return Err(XPathError(t!("tools.xpath.count-of-nodes"))),
            },
            None => Value::Text(context.item.local_name()),
        },
        _ => {
            return Err(XPathError(t!(
                "tools.xpath.unknown-function",
                name = format!("{name}()")
            )));
        }
    })
}

/// The nodes `path` selects from `context`.
fn select<'a, 'input>(
    path: &Path,
    context: &Context<'a, 'input>,
) -> Result<Vec<Item<'a, 'input>>, XPathError> {
    let start = if path.absolute {
        match context.item {
            Item::Node(node) | Item::Attribute(node, _) => {
                let root = node.document().root();
                Item::Node(root)
            }
        }
    } else {
        context.item
    };
    let mut current = vec![start];
    for step in &path.steps {
        let mut next = Vec::new();
        for item in &current {
            let candidates: Vec<Item<'a, 'input>> = along(step, *item)
                .into_iter()
                .filter(|candidate| passes(&step.test, &step.axis, candidate))
                .collect();
            if step.axis == Axis::Descendant {
                // `//book[1]` is each parent's first book: positions count among siblings.
                let mut groups: Vec<Vec<Item<'a, 'input>>> = Vec::new();
                let mut group_of = std::collections::HashMap::new();
                for candidate in candidates {
                    let parent = match candidate {
                        Item::Node(node) => node.parent().map(|parent| parent.id()),
                        Item::Attribute(node, _) => Some(node.id()),
                    };
                    let index = *group_of.entry(parent).or_insert_with(|| {
                        groups.push(Vec::new());
                        groups.len() - 1
                    });
                    groups[index].push(candidate);
                }
                for group in groups {
                    next.extend(filter(&step.predicates, group)?);
                }
            } else {
                next.extend(filter(&step.predicates, candidates)?);
            }
            if next.len() > MAX_RESULTS {
                break;
            }
        }
        sort_items(&mut next);
        current = next;
    }
    Ok(current)
}

/// The items of `items` that every predicate keeps, each predicate counting positions among
/// what the one before kept.
fn filter<'a, 'input>(
    predicates: &[Expr],
    items: Vec<Item<'a, 'input>>,
) -> Result<Vec<Item<'a, 'input>>, XPathError> {
    let mut kept = items;
    for predicate in predicates {
        let size = kept.len();
        let mut filtered = Vec::with_capacity(size);
        for (index, candidate) in kept.into_iter().enumerate() {
            let inner = Context {
                item: candidate,
                position: index + 1,
                size,
            };
            let keep = match eval(predicate, &inner)? {
                // A number alone is a position.
                Value::Number(number) => number == (index + 1) as f64,
                other => other.truth(),
            };
            if keep {
                filtered.push(candidate);
            }
        }
        kept = filtered;
    }
    Ok(kept)
}

/// The nodes a step looks at from `item`, before its test.
fn along<'a, 'input>(step: &Step, item: Item<'a, 'input>) -> Vec<Item<'a, 'input>> {
    let Item::Node(node) = item else {
        // An attribute has a parent and itself, nothing below.
        return match step.axis {
            Axis::SelfNode => vec![item],
            Axis::Parent => match item {
                Item::Attribute(node, _) => vec![Item::Node(node)],
                Item::Node(_) => Vec::new(),
            },
            _ => Vec::new(),
        };
    };
    match step.axis {
        Axis::Child => node.children().map(Item::Node).collect(),
        Axis::Descendant => node.descendants().skip(1).map(Item::Node).collect(),
        Axis::Attribute => (0..node.attributes().len())
            .map(|index| Item::Attribute(node, index))
            .collect(),
        Axis::SelfNode => vec![item],
        Axis::Parent => node.parent().map(Item::Node).into_iter().collect(),
    }
}

fn passes(test: &Test, axis: &Axis, item: &Item<'_, '_>) -> bool {
    match (item, test) {
        (Item::Attribute(..), Test::Name(name)) => {
            *axis == Axis::Attribute && name.as_ref().is_none_or(|name| *name == item.local_name())
        }
        (Item::Attribute(..), Test::Node) => true,
        (Item::Attribute(..), _) => false,
        (Item::Node(node), Test::Name(name)) => {
            node.is_element()
                && name
                    .as_ref()
                    .is_none_or(|name| name == node.tag_name().name())
        }
        (Item::Node(node), Test::Text) => node.is_text(),
        (Item::Node(node), Test::Comment) => node.is_comment(),
        (Item::Node(_), Test::Node) => true,
    }
}
