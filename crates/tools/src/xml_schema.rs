//! Validating XML, as XML Tools' "Validate now" does: against the DTD the document declares
//! (its internal subset, and the file its `SYSTEM` identifier names) or against an XML Schema
//! (the file `xsi:noNamespaceSchemaLocation` or `xsi:schemaLocation` names, or one chosen).
//!
//! What is checked, written from the XML 1.0 and XML Schema 1.0 recommendations:
//!
//! - DTD: the root's name; that elements are declared; content models (`EMPTY`, `ANY`, mixed
//!   content, sequences and choices with `?`, `*` and `+`); attributes: declared, `#REQUIRED`,
//!   `#FIXED`, enumerations, `ID` (unique), `IDREF` and `IDREFS` (pointing at an `ID`),
//!   `NMTOKEN` and `NMTOKENS`. Parameter entities are not expanded: declarations that use them
//!   are left out.
//! - XML Schema: global and local element declarations and references, named and anonymous
//!   complex and simple types, `sequence`, `choice` and `all` with `minOccurs` and `maxOccurs`,
//!   `group` and `attributeGroup` references, `any`, mixed content, simple content and complex
//!   content extending or restricting a base, attributes (`use`, `fixed`), and simple types
//!   restricting the built-in ones with `enumeration`, `pattern`, the length and the range
//!   facets, and `list`. Names are matched by their local part; `import`, `include`,
//!   substitution groups, identity constraints and `xsi:type` are not followed.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::ops::Range;

use birchpad_i18n::t;
use roxmltree::{Document, Node, ParsingOptions};

/// The most problems reported for one document.
pub const MAX_PROBLEMS: usize = 100;
/// How deep content models may nest.
const MAX_MODEL_DEPTH: usize = 64;

const XSD_NS: &str = "http://www.w3.org/2001/XMLSchema";
const XSI_NS: &str = "http://www.w3.org/2001/XMLSchema-instance";

/// Something the document does against its DTD or schema, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// Byte offset in the document.
    pub offset: usize,
    pub message: String,
}

/// A DTD or schema that cannot be used, or a document that is not XML.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct SchemaError(pub String);

/// The `<!DOCTYPE ...>` of a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Doctype {
    /// The name the root element must have.
    pub root: String,
    /// The file of the external subset (`SYSTEM "x.dtd"` or `PUBLIC "..." "x.dtd"`).
    pub system: Option<String>,
    /// The internal subset, between `[` and `]`.
    pub internal: Option<String>,
}

fn parse_document(text: &str) -> Result<Document<'_>, SchemaError> {
    let options = ParsingOptions {
        allow_dtd: true,
        ..ParsingOptions::default()
    };
    Document::parse_with_options(text, options)
        .map_err(|error| SchemaError(t!("tools.xml-schema.not-xml", reason = error)))
}

/// The document's `<!DOCTYPE ...>`, if it has one.
pub fn doctype(text: &str) -> Option<Doctype> {
    let start = text.find("<!DOCTYPE")?;
    let rest = &text[start + "<!DOCTYPE".len()..];
    let rest = rest.trim_start();
    let name_len = rest
        .find(|c: char| c.is_whitespace() || c == '[' || c == '>')
        .unwrap_or(rest.len());
    let root = rest[..name_len].to_owned();
    let mut rest = rest[name_len..].trim_start();
    let mut system = None;
    let literal = |rest: &str| -> Option<(String, usize)> {
        let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let end = rest[1..].find(quote)?;
        Some((rest[1..1 + end].to_owned(), end + 2))
    };
    if let Some(after) = rest.strip_prefix("SYSTEM") {
        let after = after.trim_start();
        if let Some((value, used)) = literal(after) {
            system = Some(value);
            rest = after[used..].trim_start();
        }
    } else if let Some(after) = rest.strip_prefix("PUBLIC") {
        let after = after.trim_start();
        if let Some((_, used)) = literal(after) {
            let after = after[used..].trim_start();
            if let Some((value, used)) = literal(after) {
                system = Some(value);
                rest = after[used..].trim_start();
            }
        }
    }
    let internal = rest.strip_prefix('[').map(|subset| {
        let mut depth_quote = None;
        let mut end = subset.len();
        for (at, c) in subset.char_indices() {
            match (depth_quote, c) {
                (None, '"' | '\'') => depth_quote = Some(c),
                (Some(open), _) if open == c => depth_quote = None,
                (None, ']') => {
                    end = at;
                    break;
                }
                _ => {}
            }
        }
        subset[..end].to_owned()
    });
    (!root.is_empty()).then_some(Doctype {
        root,
        system,
        internal,
    })
}

/// The schema file the root element names: `xsi:noNamespaceSchemaLocation`, or the location of
/// the first pair of `xsi:schemaLocation`.
pub fn schema_location(text: &str) -> Option<String> {
    let document = parse_document(text).ok()?;
    let root = document.root_element();
    if let Some(location) = root.attribute((XSI_NS, "noNamespaceSchemaLocation")) {
        return Some(location.trim().to_owned());
    }
    root.attribute((XSI_NS, "schemaLocation"))
        .and_then(|pairs| pairs.split_whitespace().nth(1))
        .map(str::to_owned)
}

// --- Content models ---------------------------------------------------------------------------

/// How many times a particle may occur; `max` `None` is unbounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Occurs {
    min: u32,
    max: Option<u32>,
}

const ONCE: Occurs = Occurs {
    min: 1,
    max: Some(1),
};

#[derive(Debug, Clone, PartialEq)]
enum Particle {
    /// An element: in a DTD its name, in a schema its declaration.
    Element(usize),
    /// Any element (`ANY`, `xs:any`).
    Any,
    Sequence(Vec<(Particle, Occurs)>),
    Choice(Vec<(Particle, Occurs)>),
    /// Each at most once, in any order (`xs:all`).
    All(Vec<(Particle, Occurs)>),
}

/// The ends a particle can reach from each of `starts` over `children` (indices of names, as
/// `matches` tells which child a particle element takes).
fn reach(
    particle: &Particle,
    occurs: Occurs,
    starts: &BTreeSet<usize>,
    children: usize,
    matches: &dyn Fn(usize, usize) -> bool,
) -> BTreeSet<usize> {
    let once = |from: &BTreeSet<usize>| -> BTreeSet<usize> {
        match particle {
            Particle::Element(element) => from
                .iter()
                .filter(|&&at| at < children && matches(*element, at))
                .map(|at| at + 1)
                .collect(),
            Particle::Any => from
                .iter()
                .filter(|&&at| at < children)
                .map(|at| at + 1)
                .collect(),
            Particle::Sequence(items) => items.iter().fold(from.clone(), |at, (item, occurs)| {
                reach(item, *occurs, &at, children, matches)
            }),
            Particle::Choice(items) => items
                .iter()
                .flat_map(|(item, occurs)| reach(item, *occurs, from, children, matches))
                .collect(),
            Particle::All(items) => from
                .iter()
                .filter_map(|&start| {
                    // Each member takes the children it matches, in any order, at most once.
                    let mut used = vec![false; items.len()];
                    let mut at = start;
                    while at < children {
                        let next = items.iter().enumerate().find(|(index, (item, _))| {
                            !used[*index]
                                && !reach(item, ONCE, &BTreeSet::from([at]), children, matches)
                                    .is_empty()
                        });
                        match next {
                            Some((index, _)) => {
                                used[index] = true;
                                at += 1;
                            }
                            None => break,
                        }
                    }
                    let complete = items
                        .iter()
                        .zip(&used)
                        .all(|((_, occurs), used)| *used || occurs.min == 0);
                    complete.then_some(at)
                })
                .collect(),
        }
    };
    let mut current = starts.clone();
    for _ in 0..occurs.min {
        current = once(&current);
        if current.is_empty() {
            return current;
        }
    }
    let mut reached = current.clone();
    let mut extra = 0;
    // Each further occurrence consumes a child at least, so this ends.
    while occurs.max.is_none_or(|max| occurs.min + extra < max) && extra as usize <= children {
        let next: BTreeSet<usize> = once(&current)
            .into_iter()
            .filter(|at| !reached.contains(at))
            .collect();
        if next.is_empty() {
            break;
        }
        reached.extend(next.iter().copied());
        current = next;
        extra += 1;
    }
    reached
}

/// Whether `children` children, as `matches` sees them, are what `model` allows.
fn fits(
    model: &(Particle, Occurs),
    children: usize,
    matches: &dyn Fn(usize, usize) -> bool,
) -> bool {
    reach(&model.0, model.1, &BTreeSet::from([0]), children, matches).contains(&children)
}

/// The elements a model names.
fn named(particle: &Particle, found: &mut Vec<usize>, any: &mut bool) {
    match particle {
        Particle::Element(element) => found.push(*element),
        Particle::Any => *any = true,
        Particle::Sequence(items) | Particle::Choice(items) | Particle::All(items) => {
            for (item, _) in items {
                named(item, found, any);
            }
        }
    }
}

// --- DTD --------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum DtdContent {
    Empty,
    Any,
    /// `#PCDATA` and these elements, in any order.
    Mixed(Vec<String>),
    Children(Particle, Occurs),
}

#[derive(Debug, Clone, PartialEq)]
enum AttributeKind {
    Text,
    Id,
    IdRef,
    IdRefs,
    Token,
    Tokens,
    Choice(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
enum AttributeDefault {
    Required,
    Implied,
    Fixed(String),
    Value,
}

#[derive(Debug, Clone, PartialEq)]
struct DtdAttribute {
    name: String,
    kind: AttributeKind,
    default: AttributeDefault,
}

#[derive(Debug, Default)]
struct Dtd {
    /// Element names, in the order declared; a particle element is an index into this.
    names: Vec<String>,
    elements: HashMap<String, DtdContent>,
    attributes: HashMap<String, Vec<DtdAttribute>>,
}

impl Dtd {
    fn index_of(&mut self, name: &str) -> usize {
        match self.names.iter().position(|known| known == name) {
            Some(index) => index,
            None => {
                self.names.push(name.to_owned());
                self.names.len() - 1
            }
        }
    }

    /// Reads the declarations of `subset`; those it cannot read are left out.
    fn read(&mut self, subset: &str) {
        let mut rest = subset;
        while let Some(start) = rest.find('<') {
            rest = &rest[start..];
            if rest.starts_with("<!--") {
                rest = rest.find("-->").map_or("", |end| &rest[end + 3..]);
                continue;
            }
            if rest.starts_with("<?") {
                rest = rest.find("?>").map_or("", |end| &rest[end + 2..]);
                continue;
            }
            let end = declaration_end(rest);
            let declaration = &rest[..end];
            rest = &rest[end..];
            if declaration.contains('%') {
                continue;
            }
            let body = declaration
                .trim_start_matches('<')
                .trim_end_matches('>')
                .trim();
            if let Some(element) = body.strip_prefix("!ELEMENT") {
                self.read_element(element);
            } else if let Some(list) = body.strip_prefix("!ATTLIST") {
                self.read_attlist(list);
            }
        }
    }

    fn read_element(&mut self, declaration: &str) {
        let declaration = declaration.trim();
        let name_len = declaration
            .find(char::is_whitespace)
            .unwrap_or(declaration.len());
        let name = declaration[..name_len].to_owned();
        let spec: String = declaration[name_len..]
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let content = match spec.as_str() {
            "EMPTY" => DtdContent::Empty,
            "ANY" => DtdContent::Any,
            spec if spec.starts_with("(#PCDATA") => DtdContent::Mixed(
                spec.trim_end_matches('*')
                    .trim_start_matches('(')
                    .trim_end_matches(')')
                    .split('|')
                    .filter(|part| *part != "#PCDATA" && !part.is_empty())
                    .map(str::to_owned)
                    .collect(),
            ),
            spec => match self.model(spec, 0) {
                Some((particle, occurs, used)) if used == spec.len() => {
                    DtdContent::Children(particle, occurs)
                }
                _ => return,
            },
        };
        self.elements.insert(name, content);
    }

    /// A content particle at the start of `spec` (without whitespace): it, how often, and the
    /// bytes it took.
    fn model(&mut self, spec: &str, depth: usize) -> Option<(Particle, Occurs, usize)> {
        if depth > MAX_MODEL_DEPTH {
            return None;
        }
        let (particle, mut used) = if let Some(inner) = spec.strip_prefix('(') {
            let mut items = Vec::new();
            let mut at = 1;
            let mut separator = None;
            loop {
                let (item, occurs, len) = self.model(&inner[at - 1..], depth + 1)?;
                items.push((item, occurs));
                at += len;
                match spec.as_bytes().get(at) {
                    Some(b')') => {
                        at += 1;
                        break;
                    }
                    Some(&c @ (b',' | b'|')) if separator.is_none_or(|s| s == c) => {
                        separator = Some(c);
                        at += 1;
                    }
                    _ => return None,
                }
            }
            let particle = if separator == Some(b'|') {
                Particle::Choice(items)
            } else {
                Particle::Sequence(items)
            };
            (particle, at)
        } else {
            let len = spec
                .find([',', '|', ')', '?', '*', '+'])
                .unwrap_or(spec.len());
            if len == 0 {
                return None;
            }
            (Particle::Element(self.index_of(&spec[..len])), len)
        };
        let occurs = match spec.as_bytes().get(used) {
            Some(b'?') => Occurs {
                min: 0,
                max: Some(1),
            },
            Some(b'*') => Occurs { min: 0, max: None },
            Some(b'+') => Occurs { min: 1, max: None },
            _ => ONCE,
        };
        if occurs != ONCE {
            used += 1;
        }
        Some((particle, occurs, used))
    }

    fn read_attlist(&mut self, list: &str) {
        let tokens = tokens(list);
        let Some(element) = tokens.first() else {
            return;
        };
        let element = element.clone();
        let mut at = 1;
        while at + 2 <= tokens.len() {
            let name = tokens[at].clone();
            let kind = match tokens[at + 1].as_str() {
                "CDATA" | "ENTITY" | "ENTITIES" => AttributeKind::Text,
                "ID" => AttributeKind::Id,
                "IDREF" => AttributeKind::IdRef,
                "IDREFS" => AttributeKind::IdRefs,
                "NMTOKEN" => AttributeKind::Token,
                "NMTOKENS" => AttributeKind::Tokens,
                "NOTATION" => {
                    at += 1;
                    AttributeKind::Choice(choices(&tokens[at + 1]))
                }
                choice if choice.starts_with('(') => AttributeKind::Choice(choices(choice)),
                _ => return,
            };
            at += 2;
            let default = match tokens.get(at).map(String::as_str) {
                Some("#REQUIRED") => AttributeDefault::Required,
                Some("#IMPLIED") => AttributeDefault::Implied,
                Some("#FIXED") => {
                    at += 1;
                    AttributeDefault::Fixed(unquote(tokens.get(at).map_or("", String::as_str)))
                }
                Some(_) => AttributeDefault::Value,
                None => return,
            };
            at += 1;
            let declared = self.attributes.entry(element.clone()).or_default();
            // The first declaration of an attribute counts.
            if !declared.iter().any(|known| known.name == name) {
                declared.push(DtdAttribute {
                    name,
                    kind,
                    default,
                });
            }
        }
    }
}

/// The length of a markup declaration, whose quoted parts may hold `>`.
fn declaration_end(rest: &str) -> usize {
    let mut quote = None;
    for (at, c) in rest.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(open), _) if open == c => quote = None,
            (None, '>') => return at + 1,
            _ => {}
        }
    }
    rest.len()
}

/// The words of an attribute list: names, types, `(a|b)` choices and quoted values whole.
fn tokens(list: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = list.char_indices().peekable();
    while let Some(&(at, c)) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        let end = match c {
            '"' | '\'' => list[at + 1..]
                .find(c)
                .map_or(list.len(), |end| at + end + 2),
            '(' => list[at..].find(')').map_or(list.len(), |end| at + end + 1),
            _ => list[at..]
                .find(|c: char| c.is_whitespace() || c == '(')
                .map_or(list.len(), |end| at + end),
        };
        tokens.push(list[at..end].chars().filter(|c| c != &'\n').collect());
        while chars.peek().is_some_and(|&(next, _)| next < end) {
            chars.next();
        }
    }
    tokens
}

fn choices(group: &str) -> Vec<String> {
    group
        .trim_start_matches('(')
        .trim_end_matches(')')
        .split('|')
        .map(|choice| choice.trim().to_owned())
        .filter(|choice| !choice.is_empty())
        .collect()
}

fn unquote(value: &str) -> String {
    value.trim_matches(|c| c == '"' || c == '\'').to_owned()
}

fn is_token(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '\u{b7}'))
}

/// Problems gathered while validating, at most [`MAX_PROBLEMS`].
#[derive(Default)]
struct Problems(Vec<Problem>);

impl Problems {
    fn push(&mut self, offset: usize, message: String) {
        if self.0.len() < MAX_PROBLEMS {
            self.0.push(Problem { offset, message });
        }
    }
}

/// Validates `text` against the DTD its `<!DOCTYPE>` declares: its internal subset and
/// `external`, the text of the file its `SYSTEM` identifier names (the internal subset's
/// declarations come first). An empty list: valid.
pub fn validate_dtd(text: &str, external: Option<&str>) -> Result<Vec<Problem>, SchemaError> {
    let document = parse_document(text)?;
    let doctype = doctype(text).ok_or_else(|| SchemaError(t!("tools.xml-schema.no-doctype")))?;
    let mut dtd = Dtd::default();
    if let Some(internal) = &doctype.internal {
        dtd.read(internal);
    }
    if let Some(external) = external {
        dtd.read(external);
    }
    let mut problems = Problems::default();
    let root = document.root_element();
    if root.tag_name().name() != doctype.root {
        problems.push(
            root.range().start,
            t!(
                "tools.xml-schema.root-not-declared",
                found = root.tag_name().name(),
                expected = doctype.root
            ),
        );
    }
    let mut ids = HashSet::new();
    let mut references: Vec<(usize, String)> = Vec::new();
    for element in root.descendants().filter(Node::is_element) {
        let name = element.tag_name().name();
        let offset = element.range().start;
        match dtd.elements.get(name) {
            None if !dtd.elements.is_empty() => problems.push(
                offset,
                t!("tools.xml-schema.element-not-declared", name = name),
            ),
            None => {}
            Some(content) => check_dtd_content(&dtd, element, content, &mut problems),
        }
        let declared = dtd.attributes.get(name).map_or(&[][..], Vec::as_slice);
        for attribute in element.attributes() {
            let attribute_name = attribute.name();
            if attribute.namespace().is_some() || attribute_name == "xmlns" {
                continue;
            }
            let Some(declaration) = declared.iter().find(|known| known.name == attribute_name)
            else {
                if !dtd.elements.is_empty() || !dtd.attributes.is_empty() {
                    problems.push(
                        attribute.range().start,
                        t!(
                            "tools.xml-schema.attribute-not-declared",
                            name = attribute_name,
                            element = name
                        ),
                    );
                }
                continue;
            };
            let value = attribute.value();
            let at = attribute.range().start;
            match &declaration.kind {
                AttributeKind::Id => {
                    if !ids.insert(value.to_owned()) {
                        problems.push(at, t!("tools.xml-schema.id-twice", id = value));
                    }
                }
                AttributeKind::IdRef => references.push((at, value.to_owned())),
                AttributeKind::IdRefs => references.extend(
                    value
                        .split_whitespace()
                        .map(|reference| (at, reference.to_owned())),
                ),
                AttributeKind::Token if !is_token(value) => problems.push(
                    at,
                    t!(
                        "tools.xml-schema.not-a-token",
                        name = attribute_name,
                        value = value
                    ),
                ),
                AttributeKind::Tokens if !value.split_whitespace().all(is_token) => problems.push(
                    at,
                    t!(
                        "tools.xml-schema.not-a-token",
                        name = attribute_name,
                        value = value
                    ),
                ),
                AttributeKind::Choice(choices) if !choices.iter().any(|choice| choice == value) => {
                    problems.push(
                        at,
                        t!(
                            "tools.xml-schema.not-a-choice",
                            name = attribute_name,
                            value = value,
                            choices = choices.join(" | ")
                        ),
                    );
                }
                _ => {}
            }
            if let AttributeDefault::Fixed(fixed) = &declaration.default
                && fixed != value
            {
                problems.push(
                    at,
                    t!(
                        "tools.xml-schema.fixed",
                        name = attribute_name,
                        value = fixed
                    ),
                );
            }
        }
        for declaration in declared {
            if declaration.default == AttributeDefault::Required
                && element.attribute(declaration.name.as_str()).is_none()
            {
                problems.push(
                    offset,
                    t!(
                        "tools.xml-schema.attribute-required",
                        name = declaration.name,
                        element = name
                    ),
                );
            }
        }
    }
    for (at, reference) in references {
        if !ids.contains(&reference) {
            problems.push(at, t!("tools.xml-schema.no-such-id", id = reference));
        }
    }
    problems.0.sort_by_key(|problem| problem.offset);
    Ok(problems.0)
}

/// The child elements of `element`, and the first text in it that is not whitespace.
fn children_of<'a, 'input>(
    element: Node<'a, 'input>,
) -> (Vec<Node<'a, 'input>>, Option<Node<'a, 'input>>) {
    let elements = element.children().filter(Node::is_element).collect();
    let text = element
        .children()
        .find(|child| child.is_text() && !child.text().unwrap_or_default().trim().is_empty());
    (elements, text)
}

fn check_dtd_content(
    dtd: &Dtd,
    element: Node<'_, '_>,
    content: &DtdContent,
    problems: &mut Problems,
) {
    let name = element.tag_name().name();
    let (children, text) = children_of(element);
    match content {
        DtdContent::Any => {}
        DtdContent::Empty => {
            if let Some(child) = children.first().copied().or(text) {
                problems.push(
                    child.range().start,
                    t!("tools.xml-schema.must-be-empty", element = name),
                );
            }
        }
        DtdContent::Mixed(allowed) => {
            for child in children {
                if !allowed.iter().any(|known| known == child.tag_name().name()) {
                    problems.push(
                        child.range().start,
                        t!(
                            "tools.xml-schema.not-allowed-here",
                            name = child.tag_name().name(),
                            element = name
                        ),
                    );
                }
            }
        }
        DtdContent::Children(particle, occurs) => {
            if let Some(text) = text {
                problems.push(
                    text.range().start,
                    t!("tools.xml-schema.no-text", element = name),
                );
            }
            let model = (particle.clone(), *occurs);
            let matches = |declared: usize, at: usize| {
                dtd.names.get(declared).map(String::as_str) == Some(children[at].tag_name().name())
            };
            if !fits(&model, children.len(), &matches) {
                let (mut known, mut any) = (Vec::new(), false);
                named(particle, &mut known, &mut any);
                let stranger = children.iter().find(|child| {
                    !any && !known
                        .iter()
                        .any(|&index| dtd.names[index] == child.tag_name().name())
                });
                match stranger {
                    Some(child) => problems.push(
                        child.range().start,
                        t!(
                            "tools.xml-schema.not-allowed-here",
                            name = child.tag_name().name(),
                            element = name
                        ),
                    ),
                    None => problems.push(
                        element.range().start,
                        t!(
                            "tools.xml-schema.children-not-as-declared",
                            element = name,
                            model = describe(particle, *occurs, &|index| dtd.names[index].clone())
                        ),
                    ),
                }
            }
        }
    }
}

/// A content model as a DTD writes it: `(a, (b | c)*, d?)`.
fn describe(particle: &Particle, occurs: Occurs, name: &dyn Fn(usize) -> String) -> String {
    let body = match particle {
        Particle::Element(index) => name(*index),
        Particle::Any => "ANY".to_owned(),
        Particle::Sequence(items) | Particle::Choice(items) | Particle::All(items) => {
            let separator = match particle {
                Particle::Sequence(_) => ", ",
                Particle::Choice(_) => " | ",
                _ => " & ",
            };
            let parts: Vec<String> = items
                .iter()
                .map(|(item, occurs)| describe(item, *occurs, name))
                .collect();
            format!("({})", parts.join(separator))
        }
    };
    let suffix = match (occurs.min, occurs.max) {
        (1, Some(1)) => "",
        (0, Some(1)) => "?",
        (0, None) => "*",
        (1, None) => "+",
        (min, Some(max)) => return format!("{body}{{{min},{max}}}"),
        (min, None) => return format!("{body}{{{min},}}"),
    };
    format!("{body}{suffix}")
}

// --- XML Schema -------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum TypeRef {
    Builtin(String),
    Simple(usize),
    Complex(usize),
    /// `xs:anyType`, or no type: anything.
    Any,
}

#[derive(Debug, Clone, PartialEq)]
struct ElementDecl {
    name: String,
    kind: TypeRef,
    fixed: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct SimpleType {
    /// The built-in type it restricts, at the bottom.
    base: String,
    /// The simple type it restricts, whose facets apply too.
    parent: Option<usize>,
    enumeration: Vec<String>,
    patterns: Vec<String>,
    length: Option<usize>,
    min_length: Option<usize>,
    max_length: Option<usize>,
    min_inclusive: Option<f64>,
    max_inclusive: Option<f64>,
    min_exclusive: Option<f64>,
    max_exclusive: Option<f64>,
    /// A list of this item type.
    list: Option<Box<TypeRef>>,
}

#[derive(Debug, Clone, PartialEq)]
struct AttributeDecl {
    name: String,
    kind: TypeRef,
    required: bool,
    prohibited: bool,
    fixed: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct ComplexType {
    mixed: bool,
    content: Option<(Particle, Occurs)>,
    /// Simple content: the type of its text.
    text: Option<TypeRef>,
    attributes: Vec<AttributeDecl>,
    any_attribute: bool,
}

#[derive(Debug, Default)]
struct Schema {
    elements: Vec<ElementDecl>,
    /// The global elements, by name.
    globals: HashMap<String, usize>,
    simple: Vec<SimpleType>,
    complex: Vec<ComplexType>,
    simple_names: HashMap<String, usize>,
    complex_names: HashMap<String, usize>,
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

fn is_xsd<'a, 'input>(node: &Node<'a, 'input>, name: &str) -> bool {
    node.is_element()
        && node.tag_name().name() == name
        && node.tag_name().namespace().is_none_or(|ns| ns == XSD_NS)
}

fn xsd_children<'a, 'input>(node: Node<'a, 'input>) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children().filter(|child| {
        child.is_element() && child.tag_name().namespace().is_none_or(|ns| ns == XSD_NS)
    })
}

fn occurs_of(node: &Node<'_, '_>) -> Occurs {
    let min = node
        .attribute("minOccurs")
        .and_then(|min| min.trim().parse().ok())
        .unwrap_or(1);
    let max = match node.attribute("maxOccurs").map(str::trim) {
        Some("unbounded") => None,
        Some(max) => Some(max.parse().unwrap_or(1)),
        None => Some(1),
    };
    Occurs { min, max }
}

const BUILTINS: &[&str] = &[
    "string",
    "normalizedString",
    "token",
    "language",
    "Name",
    "NCName",
    "QName",
    "ID",
    "IDREF",
    "IDREFS",
    "ENTITY",
    "ENTITIES",
    "NMTOKEN",
    "NMTOKENS",
    "anyURI",
    "boolean",
    "decimal",
    "integer",
    "int",
    "long",
    "short",
    "byte",
    "nonNegativeInteger",
    "positiveInteger",
    "nonPositiveInteger",
    "negativeInteger",
    "unsignedLong",
    "unsignedInt",
    "unsignedShort",
    "unsignedByte",
    "float",
    "double",
    "date",
    "dateTime",
    "time",
    "duration",
    "gYear",
    "gYearMonth",
    "gMonth",
    "gMonthDay",
    "gDay",
    "hexBinary",
    "base64Binary",
    "anySimpleType",
];

/// Reads a schema, following references to its own named parts.
struct SchemaReader<'a, 'input> {
    root: Node<'a, 'input>,
    schema: Schema,
    depth: usize,
}

impl<'a, 'input> SchemaReader<'a, 'input> {
    fn named(&self, kind: &str, name: &str) -> Option<Node<'a, 'input>> {
        xsd_children(self.root)
            .find(|child| is_xsd(child, kind) && child.attribute("name") == Some(local(name)))
    }

    fn type_ref(&mut self, name: &str) -> TypeRef {
        let name = local(name);
        if name == "anyType" {
            return TypeRef::Any;
        }
        if let Some(&index) = self.schema.complex_names.get(name) {
            return TypeRef::Complex(index);
        }
        if let Some(&index) = self.schema.simple_names.get(name) {
            return TypeRef::Simple(index);
        }
        if let Some(node) = self.named("complexType", name) {
            let index = self.schema.complex.len();
            self.schema.complex.push(ComplexType::default());
            self.schema.complex_names.insert(name.to_owned(), index);
            let read = self.complex(node);
            self.schema.complex[index] = read;
            return TypeRef::Complex(index);
        }
        if let Some(node) = self.named("simpleType", name) {
            let index = self.schema.simple.len();
            self.schema.simple.push(SimpleType::default());
            self.schema.simple_names.insert(name.to_owned(), index);
            let read = self.simple(node);
            self.schema.simple[index] = read;
            return TypeRef::Simple(index);
        }
        if BUILTINS.contains(&name) {
            return TypeRef::Builtin(name.to_owned());
        }
        TypeRef::Any
    }

    /// The type of an element or an attribute: its `type`, or the type inside it.
    fn type_of(&mut self, node: Node<'a, 'input>) -> TypeRef {
        if let Some(name) = node.attribute("type") {
            return self.type_ref(name);
        }
        if let Some(inner) = xsd_children(node).find(|child| is_xsd(child, "complexType")) {
            let read = self.complex(inner);
            self.schema.complex.push(read);
            return TypeRef::Complex(self.schema.complex.len() - 1);
        }
        if let Some(inner) = xsd_children(node).find(|child| is_xsd(child, "simpleType")) {
            let read = self.simple(inner);
            self.schema.simple.push(read);
            return TypeRef::Simple(self.schema.simple.len() - 1);
        }
        TypeRef::Any
    }

    fn element(&mut self, node: Node<'a, 'input>) -> usize {
        if let Some(reference) = node.attribute("ref") {
            let name = local(reference);
            if let Some(&index) = self.schema.globals.get(name) {
                return index;
            }
            if let Some(global) = self.named("element", name) {
                return self.global(global);
            }
        }
        let index = self.schema.elements.len();
        self.schema.elements.push(ElementDecl {
            name: node.attribute("name").unwrap_or_default().to_owned(),
            kind: TypeRef::Any,
            fixed: node.attribute("fixed").map(str::to_owned),
        });
        if self.depth < MAX_MODEL_DEPTH {
            self.depth += 1;
            let kind = self.type_of(node);
            self.depth -= 1;
            self.schema.elements[index].kind = kind;
        }
        index
    }

    fn global(&mut self, node: Node<'a, 'input>) -> usize {
        let name = node.attribute("name").unwrap_or_default().to_owned();
        if let Some(&index) = self.schema.globals.get(&name) {
            return index;
        }
        let index = self.schema.elements.len();
        self.schema.globals.insert(name.clone(), index);
        self.schema.elements.push(ElementDecl {
            name,
            kind: TypeRef::Any,
            fixed: node.attribute("fixed").map(str::to_owned),
        });
        let kind = self.type_of(node);
        self.schema.elements[index].kind = kind;
        index
    }

    /// A particle: an element, `any`, a group reference, or a model group.
    fn particle(&mut self, node: Node<'a, 'input>) -> Option<(Particle, Occurs)> {
        if self.depth > MAX_MODEL_DEPTH {
            return None;
        }
        self.depth += 1;
        let occurs = occurs_of(&node);
        let particle = match node.tag_name().name() {
            "element" => Some(Particle::Element(self.element(node))),
            "any" => Some(Particle::Any),
            "group" => node.attribute("ref").and_then(|reference| {
                let group = self.named("group", reference)?;
                let inner = xsd_children(group).find(|child| {
                    matches!(child.tag_name().name(), "sequence" | "choice" | "all")
                })?;
                self.particle(inner).map(|(particle, _)| particle)
            }),
            kind @ ("sequence" | "choice" | "all") => {
                let items = xsd_children(node)
                    .filter_map(|child| self.particle(child))
                    .collect();
                Some(match kind {
                    "sequence" => Particle::Sequence(items),
                    "choice" => Particle::Choice(items),
                    _ => Particle::All(items),
                })
            }
            _ => None,
        };
        self.depth -= 1;
        particle.map(|particle| (particle, occurs))
    }

    fn attributes(&mut self, node: Node<'a, 'input>, into: &mut ComplexType) {
        for child in xsd_children(node) {
            match child.tag_name().name() {
                "attribute" => {
                    let declaration = match child.attribute("ref") {
                        Some(reference) => self.named("attribute", reference).unwrap_or(child),
                        None => child,
                    };
                    let kind = self.type_of(declaration);
                    let usage = child.attribute("use").unwrap_or("optional");
                    into.attributes.push(AttributeDecl {
                        name: local(declaration.attribute("name").unwrap_or_default()).to_owned(),
                        kind,
                        required: usage == "required",
                        prohibited: usage == "prohibited",
                        fixed: child
                            .attribute("fixed")
                            .or_else(|| declaration.attribute("fixed"))
                            .map(str::to_owned),
                    });
                }
                "attributeGroup" => {
                    if let Some(group) = child
                        .attribute("ref")
                        .and_then(|reference| self.named("attributeGroup", reference))
                        && self.depth < MAX_MODEL_DEPTH
                    {
                        self.depth += 1;
                        self.attributes(group, into);
                        self.depth -= 1;
                    }
                }
                "anyAttribute" => into.any_attribute = true,
                _ => {}
            }
        }
    }

    fn complex(&mut self, node: Node<'a, 'input>) -> ComplexType {
        let mut read = ComplexType {
            mixed: node.attribute("mixed") == Some("true"),
            ..ComplexType::default()
        };
        if self.depth > MAX_MODEL_DEPTH {
            read.content = Some((Particle::Any, Occurs { min: 0, max: None }));
            read.any_attribute = true;
            return read;
        }
        self.depth += 1;
        for child in xsd_children(node) {
            match child.tag_name().name() {
                "sequence" | "choice" | "all" | "group" => read.content = self.particle(child),
                "simpleContent" | "complexContent" => {
                    let simple = child.tag_name().name() == "simpleContent";
                    if child.attribute("mixed") == Some("true") {
                        read.mixed = true;
                    }
                    let Some(derivation) = xsd_children(child).find(|inner| {
                        matches!(inner.tag_name().name(), "extension" | "restriction")
                    }) else {
                        continue;
                    };
                    let extends = derivation.tag_name().name() == "extension";
                    let base = derivation
                        .attribute("base")
                        .map(|base| self.type_ref(base))
                        .unwrap_or(TypeRef::Any);
                    if simple {
                        read.text = Some(match &base {
                            TypeRef::Complex(index) => self.schema.complex[*index]
                                .text
                                .clone()
                                .unwrap_or(TypeRef::Any),
                            other => other.clone(),
                        });
                    }
                    if let TypeRef::Complex(index) = &base {
                        let inherited = self.schema.complex[*index].clone();
                        read.attributes.extend(inherited.attributes);
                        read.any_attribute |= inherited.any_attribute;
                        if extends && !simple {
                            read.content = inherited.content;
                            read.mixed |= inherited.mixed;
                        }
                    }
                    let own = xsd_children(derivation)
                        .find(|inner| {
                            matches!(
                                inner.tag_name().name(),
                                "sequence" | "choice" | "all" | "group"
                            )
                        })
                        .and_then(|inner| self.particle(inner));
                    read.content = match (read.content.take(), own) {
                        (Some(base), Some(own)) => {
                            Some((Particle::Sequence(vec![base, own]), ONCE))
                        }
                        (base, own) => own.or(base),
                    };
                    let mut own_attributes = ComplexType::default();
                    self.attributes(derivation, &mut own_attributes);
                    // A restriction's own declarations take the place of the base's.
                    for attribute in own_attributes.attributes {
                        read.attributes.retain(|known| known.name != attribute.name);
                        read.attributes.push(attribute);
                    }
                    read.any_attribute |= own_attributes.any_attribute;
                }
                _ => {}
            }
        }
        self.attributes(node, &mut read);
        self.depth -= 1;
        read
    }

    fn simple(&mut self, node: Node<'a, 'input>) -> SimpleType {
        let mut read = SimpleType {
            base: "string".to_owned(),
            ..SimpleType::default()
        };
        if self.depth > MAX_MODEL_DEPTH {
            return read;
        }
        self.depth += 1;
        for child in xsd_children(node) {
            match child.tag_name().name() {
                "restriction" => {
                    let base = match child.attribute("base") {
                        Some(base) => self.type_ref(base),
                        None => self.type_of(child),
                    };
                    match base {
                        TypeRef::Builtin(name) => read.base = name,
                        TypeRef::Simple(index) => {
                            read.base = self.schema.simple[index].base.clone();
                            read.parent = Some(index);
                        }
                        _ => {}
                    }
                    for facet in xsd_children(child) {
                        let value = facet.attribute("value").unwrap_or_default();
                        let number = || value.trim().parse::<f64>().ok();
                        let count = || value.trim().parse::<usize>().ok();
                        match facet.tag_name().name() {
                            "enumeration" => read.enumeration.push(value.to_owned()),
                            "pattern" => read.patterns.push(value.to_owned()),
                            "length" => read.length = count(),
                            "minLength" => read.min_length = count(),
                            "maxLength" => read.max_length = count(),
                            "minInclusive" => read.min_inclusive = number(),
                            "maxInclusive" => read.max_inclusive = number(),
                            "minExclusive" => read.min_exclusive = number(),
                            "maxExclusive" => read.max_exclusive = number(),
                            _ => {}
                        }
                    }
                }
                "list" => {
                    let item = match child.attribute("itemType") {
                        Some(item) => self.type_ref(item),
                        None => self.type_of(child),
                    };
                    read.list = Some(Box::new(item));
                }
                _ => {}
            }
        }
        self.depth -= 1;
        read
    }
}

/// Reads the schema `text`.
fn read_schema<'a, 'input>(document: &'a Document<'input>) -> Result<Schema, SchemaError> {
    let root = document.root_element();
    if !is_xsd(&root, "schema") {
        return Err(SchemaError(t!("tools.xml-schema.not-a-schema")));
    }
    let mut reader = SchemaReader {
        root,
        schema: Schema::default(),
        depth: 0,
    };
    for child in xsd_children(root).filter(|child| is_xsd(child, "element")) {
        reader.global(child);
    }
    Ok(reader.schema)
}

/// Why `value` is not of the built-in type `name`, if it is not.
fn builtin_problem(name: &str, value: &str) -> Option<String> {
    let value = value.trim();
    let integer = |min: i128, max: i128| -> bool {
        let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
        !digits.is_empty()
            && digits.bytes().all(|b| b.is_ascii_digit())
            && value
                .parse::<i128>()
                .is_ok_and(|number| (min..=max).contains(&number))
    };
    let date = |value: &str| -> bool {
        let value = value.strip_prefix('-').unwrap_or(value);
        let value = strip_timezone(value);
        let parts: Vec<&str> = value.split('-').collect();
        parts.len() == 3
            && parts[0].len() >= 4
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
            && parts[1].len() == 2
            && parts[2].len() == 2
            && (1..=12).contains(&parts[1].parse::<u32>().unwrap_or(0))
            && (1..=31).contains(&parts[2].parse::<u32>().unwrap_or(0))
    };
    let time = |value: &str| -> bool {
        let value = strip_timezone(value);
        let parts: Vec<&str> = value.split(':').collect();
        parts.len() == 3
            && parts[0].len() == 2
            && parts[1].len() == 2
            && parts[0].parse::<u32>().is_ok_and(|hours| hours <= 24)
            && parts[1].parse::<u32>().is_ok_and(|minutes| minutes < 60)
            && parts[2]
                .parse::<f64>()
                .is_ok_and(|seconds| (0.0..61.0).contains(&seconds))
            && parts[2].starts_with(|c: char| c.is_ascii_digit())
    };
    let ok = match name {
        "boolean" => matches!(value, "true" | "false" | "1" | "0"),
        "decimal" => {
            let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
            let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
            !(whole.is_empty() && fraction.is_empty())
                && whole.bytes().all(|b| b.is_ascii_digit())
                && fraction.bytes().all(|b| b.is_ascii_digit())
        }
        "integer" => integer(i128::MIN, i128::MAX),
        "long" => integer(i64::MIN.into(), i64::MAX.into()),
        "int" => integer(i32::MIN.into(), i32::MAX.into()),
        "short" => integer(i16::MIN.into(), i16::MAX.into()),
        "byte" => integer(i8::MIN.into(), i8::MAX.into()),
        "nonNegativeInteger" => integer(0, i128::MAX),
        "positiveInteger" => integer(1, i128::MAX),
        "nonPositiveInteger" => integer(i128::MIN, 0),
        "negativeInteger" => integer(i128::MIN, -1),
        "unsignedLong" => integer(0, u64::MAX.into()),
        "unsignedInt" => integer(0, u32::MAX.into()),
        "unsignedShort" => integer(0, u16::MAX.into()),
        "unsignedByte" => integer(0, u8::MAX.into()),
        "float" | "double" => {
            matches!(value, "INF" | "-INF" | "NaN")
                || (value.parse::<f64>().is_ok() && !value.contains(['i', 'I', 'n', 'N']))
        }
        "date" => date(value),
        "time" => time(value),
        "dateTime" => value
            .split_once('T')
            .is_some_and(|(day, hour)| date(day) && time(hour)),
        "NMTOKEN" => is_token(value),
        "NMTOKENS" => value.split_whitespace().all(is_token) && !value.is_empty(),
        "Name" | "NCName" | "ID" | "IDREF" | "QName" => {
            is_token(value)
                && !value.starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '.')
                && (name == "Name" || name == "QName" || !value.contains(':'))
        }
        "hexBinary" => {
            value.len().is_multiple_of(2) && value.bytes().all(|b| b.is_ascii_hexdigit())
        }
        _ => true,
    };
    (!ok).then(|| t!("tools.xml-schema.not-of-type", value = value, kind = name))
}

fn strip_timezone(value: &str) -> &str {
    if let Some(value) = value.strip_suffix('Z') {
        return value;
    }
    // `+hh:mm` or `-hh:mm` at the end.
    if value.len() > 6 {
        let (head, zone) = value.split_at(value.len() - 6);
        if zone.starts_with(['+', '-']) && zone.as_bytes()[3] == b':' {
            return head;
        }
    }
    value
}

/// An XML Schema pattern as a regular expression of the whole value.
fn pattern_regex(pattern: &str) -> Option<fancy_regex::Regex> {
    // XML Schema's name classes, as near as plain classes come.
    let translated = pattern
        .replace("\\i", "[A-Za-z_:]")
        .replace("\\I", "[^A-Za-z_:]")
        .replace("\\c", "[-.0-9A-Za-z_:]")
        .replace("\\C", "[^-.0-9A-Za-z_:]");
    fancy_regex::Regex::new(&format!("^(?:{translated})$")).ok()
}

impl Schema {
    /// Why `value` is not of `kind`, if it is not.
    fn value_problem(&self, kind: &TypeRef, value: &str, depth: usize) -> Option<String> {
        if depth > MAX_MODEL_DEPTH {
            return None;
        }
        match kind {
            TypeRef::Any | TypeRef::Complex(_) => None,
            TypeRef::Builtin(name) => builtin_problem(name, value),
            TypeRef::Simple(index) => {
                let simple = &self.simple[*index];
                if let Some(item) = &simple.list {
                    return value
                        .split_whitespace()
                        .find_map(|part| self.value_problem(item, part, depth + 1));
                }
                if let Some(problem) = simple
                    .parent
                    .and_then(|parent| {
                        self.value_problem(&TypeRef::Simple(parent), value, depth + 1)
                    })
                    .or_else(|| builtin_problem(&simple.base, value))
                {
                    return Some(problem);
                }
                let compared = if simple.base.contains("string") || simple.base == "anyURI" {
                    value
                } else {
                    value.trim()
                };
                if !simple.enumeration.is_empty()
                    && !simple.enumeration.iter().any(|choice| choice == compared)
                {
                    return Some(t!(
                        "tools.xml-schema.not-a-choice",
                        name = "",
                        value = compared,
                        choices = simple.enumeration.join(" | ")
                    ));
                }
                for pattern in &simple.patterns {
                    if pattern_regex(pattern)
                        .is_some_and(|regex| !regex.is_match(compared).unwrap_or(true))
                    {
                        return Some(t!(
                            "tools.xml-schema.not-like-pattern",
                            value = compared,
                            pattern = pattern
                        ));
                    }
                }
                let length = compared.chars().count();
                let length_ok = simple.length.is_none_or(|exact| length == exact)
                    && simple.min_length.is_none_or(|min| length >= min)
                    && simple.max_length.is_none_or(|max| length <= max);
                if !length_ok {
                    return Some(t!("tools.xml-schema.wrong-length", value = compared));
                }
                let bounded = simple.min_inclusive.is_some()
                    || simple.max_inclusive.is_some()
                    || simple.min_exclusive.is_some()
                    || simple.max_exclusive.is_some();
                if bounded {
                    let number = compared.parse::<f64>().unwrap_or(f64::NAN);
                    let in_range = simple.min_inclusive.is_none_or(|min| number >= min)
                        && simple.max_inclusive.is_none_or(|max| number <= max)
                        && simple.min_exclusive.is_none_or(|min| number > min)
                        && simple.max_exclusive.is_none_or(|max| number < max);
                    if !in_range {
                        return Some(t!("tools.xml-schema.out-of-range", value = compared));
                    }
                }
                None
            }
        }
    }

    fn check_element(
        &self,
        element: Node<'_, '_>,
        declaration: usize,
        problems: &mut Problems,
        depth: usize,
    ) {
        if depth > 10_000 || problems.0.len() >= MAX_PROBLEMS {
            return;
        }
        let declared = &self.elements[declaration];
        let name = element.tag_name().name();
        let offset = element.range().start;
        let (children, text) = children_of(element);
        let own_attributes = || {
            element.attributes().filter(|attribute| {
                attribute.namespace() != Some(XSI_NS) && attribute.name() != "xmlns"
            })
        };
        let text_value: String = element
            .children()
            .filter(Node::is_text)
            .filter_map(|text| text.text())
            .collect();
        if let Some(fixed) = &declared.fixed
            && text_value.trim() != fixed.trim()
        {
            problems.push(
                offset,
                t!("tools.xml-schema.fixed", name = name, value = fixed),
            );
        }
        match &declared.kind {
            TypeRef::Any => {
                // Anything below is laxly checked: what the schema declares globally.
                for child in children {
                    if let Some(&global) = self.globals.get(child.tag_name().name()) {
                        self.check_element(child, global, problems, depth + 1);
                    }
                }
            }
            TypeRef::Builtin(_) | TypeRef::Simple(_) => {
                if let Some(child) = children.first() {
                    problems.push(
                        child.range().start,
                        t!(
                            "tools.xml-schema.not-allowed-here",
                            name = child.tag_name().name(),
                            element = name
                        ),
                    );
                }
                if let Some(attribute) = own_attributes().next() {
                    problems.push(
                        attribute.range().start,
                        t!(
                            "tools.xml-schema.attribute-not-declared",
                            name = attribute.name(),
                            element = name
                        ),
                    );
                }
                if let Some(problem) = self.value_problem(&declared.kind, &text_value, 0) {
                    problems.push(offset, format!("<{name}>: {problem}"));
                }
            }
            TypeRef::Complex(index) => {
                let complex = &self.complex[*index];
                for attribute in own_attributes() {
                    match complex
                        .attributes
                        .iter()
                        .find(|known| known.name == attribute.name())
                    {
                        Some(known) if known.prohibited => problems.push(
                            attribute.range().start,
                            t!(
                                "tools.xml-schema.attribute-not-declared",
                                name = attribute.name(),
                                element = name
                            ),
                        ),
                        Some(known) => {
                            if let Some(problem) =
                                self.value_problem(&known.kind, attribute.value(), 0)
                            {
                                problems.push(
                                    attribute.range().start,
                                    format!("@{}: {problem}", attribute.name()),
                                );
                            }
                            if let Some(fixed) = &known.fixed
                                && fixed != attribute.value()
                            {
                                problems.push(
                                    attribute.range().start,
                                    t!(
                                        "tools.xml-schema.fixed",
                                        name = attribute.name(),
                                        value = fixed
                                    ),
                                );
                            }
                        }
                        None if complex.any_attribute => {}
                        None => problems.push(
                            attribute.range().start,
                            t!(
                                "tools.xml-schema.attribute-not-declared",
                                name = attribute.name(),
                                element = name
                            ),
                        ),
                    }
                }
                for known in complex.attributes.iter().filter(|known| known.required) {
                    if element.attribute(known.name.as_str()).is_none() {
                        problems.push(
                            offset,
                            t!(
                                "tools.xml-schema.attribute-required",
                                name = known.name,
                                element = name
                            ),
                        );
                    }
                }
                if let Some(kind) = &complex.text {
                    if let Some(child) = children.first() {
                        problems.push(
                            child.range().start,
                            t!(
                                "tools.xml-schema.not-allowed-here",
                                name = child.tag_name().name(),
                                element = name
                            ),
                        );
                    }
                    if let Some(problem) = self.value_problem(kind, &text_value, 0) {
                        problems.push(offset, format!("<{name}>: {problem}"));
                    }
                    return;
                }
                if let Some(text) = text
                    && !complex.mixed
                {
                    problems.push(
                        text.range().start,
                        t!("tools.xml-schema.no-text", element = name),
                    );
                }
                let model = complex
                    .content
                    .clone()
                    .unwrap_or((Particle::Sequence(Vec::new()), ONCE));
                let matches = |element: usize, at: usize| {
                    self.elements[element].name == children[at].tag_name().name()
                };
                let (mut known, mut any) = (Vec::new(), false);
                named(&model.0, &mut known, &mut any);
                if !fits(&model, children.len(), &matches) {
                    let stranger = children.iter().find(|child| {
                        !any && !known
                            .iter()
                            .any(|&index| self.elements[index].name == child.tag_name().name())
                    });
                    match stranger {
                        Some(child) => problems.push(
                            child.range().start,
                            t!(
                                "tools.xml-schema.not-allowed-here",
                                name = child.tag_name().name(),
                                element = name
                            ),
                        ),
                        None => problems.push(
                            offset,
                            t!(
                                "tools.xml-schema.children-not-as-declared",
                                element = name,
                                model = describe(&model.0, model.1, &|index| self.elements[index]
                                    .name
                                    .clone())
                            ),
                        ),
                    }
                }
                for child in children {
                    let child_name = child.tag_name().name();
                    match known
                        .iter()
                        .find(|&&index| self.elements[index].name == child_name)
                    {
                        Some(&index) => self.check_element(child, index, problems, depth + 1),
                        None => {
                            if let Some(&global) = self.globals.get(child_name) {
                                self.check_element(child, global, problems, depth + 1);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Validates `text` against the XML Schema `schema`. An empty list: valid.
pub fn validate_xsd(text: &str, schema: &str) -> Result<Vec<Problem>, SchemaError> {
    let document = parse_document(text)?;
    let schema_document = parse_document(schema)
        .map_err(|error| SchemaError(t!("tools.xml-schema.schema-not-xml", reason = error)))?;
    let schema = read_schema(&schema_document)?;
    let root = document.root_element();
    let mut problems = Problems::default();
    match schema.globals.get(root.tag_name().name()) {
        Some(&declaration) => schema.check_element(root, declaration, &mut problems, 0),
        None => problems.push(
            root.range().start,
            t!(
                "tools.xml-schema.root-not-in-schema",
                name = root.tag_name().name()
            ),
        ),
    }
    problems.0.sort_by_key(|problem| problem.offset);
    Ok(problems.0)
}

/// The range of the start tag of the element at `offset`, for showing a problem.
pub fn tag_at(text: &str, offset: usize) -> Range<usize> {
    let start = offset.min(text.len());
    let end = text[start..]
        .find('>')
        .map_or(text.len(), |end| start + end + 1);
    start..end
}
