//! Validating against a DTD: its declarations read from the internal subset and the external
//! file, then each element checked.

use std::collections::{HashMap, HashSet};

use birchpad_i18n::t;
use roxmltree::Node;

use super::content_model::{ONCE, Occurs, Particle, fits, named};
use super::{MAX_MODEL_DEPTH, Problem, Problems, SchemaError, doctype, parse_document};

#[derive(Debug, Clone, PartialEq)]
pub(super) enum DtdContent {
    Empty,
    Any,
    /// `#PCDATA` and these elements, in any order.
    Mixed(Vec<String>),
    Children(Particle, Occurs),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum AttributeKind {
    Text,
    Id,
    IdRef,
    IdRefs,
    Token,
    Tokens,
    Choice(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum AttributeDefault {
    Required,
    Implied,
    Fixed(String),
    Value,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct DtdAttribute {
    name: String,
    kind: AttributeKind,
    default: AttributeDefault,
}

#[derive(Debug, Default)]
pub(super) struct Dtd {
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
pub(super) fn declaration_end(rest: &str) -> usize {
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
pub(super) fn tokens(list: &str) -> Vec<String> {
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

pub(super) fn choices(group: &str) -> Vec<String> {
    group
        .trim_start_matches('(')
        .trim_end_matches(')')
        .split('|')
        .map(|choice| choice.trim().to_owned())
        .filter(|choice| !choice.is_empty())
        .collect()
}

pub(super) fn unquote(value: &str) -> String {
    value.trim_matches(|c| c == '"' || c == '\'').to_owned()
}

pub(super) fn is_token(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '\u{b7}'))
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
pub(super) fn children_of<'a, 'input>(
    element: Node<'a, 'input>,
) -> (Vec<Node<'a, 'input>>, Option<Node<'a, 'input>>) {
    let elements = element.children().filter(Node::is_element).collect();
    let text = element
        .children()
        .find(|child| child.is_text() && !child.text().unwrap_or_default().trim().is_empty());
    (elements, text)
}

pub(super) fn check_dtd_content(
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
pub(super) fn describe(
    particle: &Particle,
    occurs: Occurs,
    name: &dyn Fn(usize) -> String,
) -> String {
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
