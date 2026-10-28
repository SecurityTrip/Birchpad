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

mod content_model;
mod dtd;
mod schema;

pub use dtd::validate_dtd;
pub use schema::validate_xsd;

use std::ops::Range;

use birchpad_i18n::t;
use roxmltree::{Document, ParsingOptions};

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

/// The range of the start tag of the element at `offset`, for showing a problem.
pub fn tag_at(text: &str, offset: usize) -> Range<usize> {
    let start = offset.min(text.len());
    let end = text[start..]
        .find('>')
        .map_or(text.len(), |end| start + end + 1);
    start..end
}
