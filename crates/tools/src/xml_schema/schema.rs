//! Validating against an XML Schema: its declarations and types read, then each element checked,
//! and the values of the built-in simple types.

mod builtins;
mod reader;

use builtins::{builtin_problem, pattern_regex};
use reader::read_schema;

use std::collections::HashMap;

use birchpad_i18n::t;
use roxmltree::Node;

use super::content_model::{ONCE, Occurs, Particle, fits, named};
use super::dtd::{children_of, describe};
use super::{
    MAX_MODEL_DEPTH, MAX_PROBLEMS, Problem, Problems, SchemaError, XSD_NS, XSI_NS, parse_document,
};

#[derive(Debug, Clone, PartialEq)]
pub(super) enum TypeRef {
    Builtin(String),
    Simple(usize),
    Complex(usize),
    /// `xs:anyType`, or no type: anything.
    Any,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ElementDecl {
    name: String,
    kind: TypeRef,
    fixed: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(super) struct SimpleType {
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
pub(super) struct AttributeDecl {
    name: String,
    kind: TypeRef,
    required: bool,
    prohibited: bool,
    fixed: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(super) struct ComplexType {
    mixed: bool,
    content: Option<(Particle, Occurs)>,
    /// Simple content: the type of its text.
    text: Option<TypeRef>,
    attributes: Vec<AttributeDecl>,
    any_attribute: bool,
}

#[derive(Debug, Default)]
pub(super) struct Schema {
    elements: Vec<ElementDecl>,
    /// The global elements, by name.
    globals: HashMap<String, usize>,
    simple: Vec<SimpleType>,
    complex: Vec<ComplexType>,
    simple_names: HashMap<String, usize>,
    complex_names: HashMap<String, usize>,
}

pub(super) fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

pub(super) fn is_xsd<'a, 'input>(node: &Node<'a, 'input>, name: &str) -> bool {
    node.is_element()
        && node.tag_name().name() == name
        && node.tag_name().namespace().is_none_or(|ns| ns == XSD_NS)
}

pub(super) fn xsd_children<'a, 'input>(
    node: Node<'a, 'input>,
) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children().filter(|child| {
        child.is_element() && child.tag_name().namespace().is_none_or(|ns| ns == XSD_NS)
    })
}

pub(super) fn occurs_of(node: &Node<'_, '_>) -> Occurs {
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
