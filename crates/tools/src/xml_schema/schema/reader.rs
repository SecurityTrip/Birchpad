//! Reading a schema: its global declarations, named types and groups, local ones as they come.

use birchpad_i18n::t;
use roxmltree::{Document, Node};

use crate::xml_schema::content_model::{ONCE, Occurs, Particle};
use crate::xml_schema::{MAX_MODEL_DEPTH, SchemaError};

use super::builtins::BUILTINS;
use super::{
    AttributeDecl, ComplexType, ElementDecl, Schema, SimpleType, TypeRef, is_xsd, local, occurs_of,
    xsd_children,
};

/// Reads a schema, following references to its own named parts.
pub(super) struct SchemaReader<'a, 'input> {
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
pub(super) fn read_schema<'a, 'input>(
    document: &'a Document<'input>,
) -> Result<Schema, SchemaError> {
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
