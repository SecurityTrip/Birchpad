//! A user defined language as its file writes it, for the Define your language dialog: the
//! lists as text, the settings, and what the dialog does not edit (styles, other attributes)
//! kept as it was read, so that saving changes only what was edited.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::{LineCommentAt, UdlError, words};

/// The lists the dialog edits, by their names in UDL files, besides "Comments" and
/// "Delimiters", which are coded.
pub const LISTS: [&str; 26] = [
    "Keywords1",
    "Keywords2",
    "Keywords3",
    "Keywords4",
    "Keywords5",
    "Keywords6",
    "Keywords7",
    "Keywords8",
    "Operators1",
    "Operators2",
    "Folders in code1, open",
    "Folders in code1, middle",
    "Folders in code1, close",
    "Folders in code2, open",
    "Folders in code2, middle",
    "Folders in code2, close",
    "Folders in comment, open",
    "Folders in comment, middle",
    "Folders in comment, close",
    "Numbers, prefix1",
    "Numbers, prefix2",
    "Numbers, extras1",
    "Numbers, extras2",
    "Numbers, suffix1",
    "Numbers, suffix2",
    "Numbers, range",
];

/// How many delimiters a language has.
pub const DELIMITERS: u8 = 8;

/// A user defined language as its file writes it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UdlDefinition {
    pub name: String,
    /// The extensions, separated by spaces (`mini mn`).
    pub extensions: String,
    pub ignore_case: bool,
    pub line_comment_at: LineCommentAt,
    /// Whether Keywords1 to 8 match words that start with them.
    pub prefix: [bool; 8],
    /// Every list by its name in the file, as written; "Comments" and "Delimiters" in their
    /// coded form.
    lists: BTreeMap<String, String>,
    /// Attributes of `<UserLang>` and `<Global>` other than those above, as read.
    lang_attributes: Vec<(String, String)>,
    global_attributes: Vec<(String, String)>,
    /// Each `<WordsStyle>`'s attributes, as read.
    styles: Vec<Vec<(String, String)>>,
}

/// The UDL version Birchpad writes.
const UDL_VERSION: &str = "2.1";

impl UdlDefinition {
    /// A new language with nothing defined.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            ..Self::default()
        }
    }

    /// The languages of a UDL file, as they are written.
    pub fn parse_all(xml: &str) -> Result<Vec<Self>, UdlError> {
        let xml = xml.strip_prefix('\u{feff}').unwrap_or(xml);
        let document =
            roxmltree::Document::parse(xml).map_err(|error| UdlError(error.to_string()))?;
        let languages: Vec<Self> = document
            .descendants()
            .filter(|node| node.has_tag_name("UserLang"))
            .filter_map(Self::of_node)
            .collect();
        if languages.is_empty() {
            return Err(UdlError("no <UserLang> with a name".into()));
        }
        Ok(languages)
    }

    fn of_node(node: roxmltree::Node) -> Option<Self> {
        let name = node.attribute("name")?.trim();
        if name.is_empty() {
            return None;
        }
        let others = |node: roxmltree::Node, known: &[&str]| {
            node.attributes()
                .filter(|attribute| !known.contains(&attribute.name()))
                .map(|attribute| (attribute.name().to_owned(), attribute.value().to_owned()))
                .collect::<Vec<_>>()
        };
        let mut definition = Self {
            name: name.to_owned(),
            extensions: node.attribute("ext").unwrap_or_default().trim().to_owned(),
            lang_attributes: others(node, &["name", "ext", "udlVersion"]),
            ..Self::default()
        };
        for child in node.descendants() {
            if child.has_tag_name("Global") {
                definition.ignore_case = child.attribute("caseIgnored") == Some("yes");
                definition.line_comment_at = match child.attribute("forcePureLC") {
                    Some("1") => LineCommentAt::LineStartOrIndent,
                    Some("2") => LineCommentAt::LineStart,
                    _ => LineCommentAt::Anywhere,
                };
                definition.global_attributes = others(child, &["caseIgnored", "forcePureLC"]);
            } else if child.has_tag_name("Prefix") {
                for (index, prefix) in definition.prefix.iter_mut().enumerate() {
                    *prefix =
                        child.attribute(format!("Keywords{}", index + 1).as_str()) == Some("yes");
                }
            } else if child.has_tag_name("Keywords")
                && let Some(list) = child.attribute("name")
            {
                definition.lists.insert(
                    list.to_owned(),
                    child.text().unwrap_or_default().trim().to_owned(),
                );
            } else if child.has_tag_name("WordsStyle") {
                definition.styles.push(others(child, &[]));
            }
        }
        Some(definition)
    }

    /// The list `name` as written.
    pub fn list(&self, name: &str) -> &str {
        self.lists.get(name).map_or("", String::as_str)
    }

    pub fn set_list(&mut self, name: &str, value: &str) {
        let value = value.trim();
        if value.is_empty() {
            self.lists.remove(name);
        } else {
            self.lists.insert(name.to_owned(), value.to_owned());
        }
    }

    fn coded(&self, name: &str) -> Vec<(u8, String)> {
        super::coded(self.list(name))
    }

    /// The line comment markers, separated by spaces, and the block comment's open and close.
    pub fn comments(&self) -> (String, String, String) {
        let codes = self.coded("Comments");
        let of = |code: u8| {
            codes
                .iter()
                .filter(move |(found, value)| *found == code && !value.is_empty())
                .map(|(_, value)| value.clone())
        };
        (
            of(0).collect::<Vec<_>>().join(" "),
            of(3).next().unwrap_or_default(),
            of(4).next().unwrap_or_default(),
        )
    }

    /// Sets the comments; `line` holds the line comment markers separated by spaces.
    pub fn set_comments(&mut self, line: &str, open: &str, close: &str) {
        let mut coded: Vec<String> = words(line).iter().map(|open| format!("00{open}")).collect();
        coded.push("01".into());
        coded.push("02".into());
        coded.push(format!("03{}", open.trim()));
        coded.push(format!("04{}", close.trim()));
        let has_any = !line.trim().is_empty() || !open.trim().is_empty();
        let value = if has_any {
            coded.join(" ")
        } else {
            String::new()
        };
        self.set_list("Comments", &value);
    }

    /// Delimiter `index` (0 to 7): what opens it, its escape character, what closes it.
    pub fn delimiter(&self, index: u8) -> (String, String, String) {
        let codes = self.coded("Delimiters");
        let part = |part: u8| {
            codes
                .iter()
                .find(|(code, _)| *code == index * 3 + part)
                .map(|(_, value)| value.clone())
                .unwrap_or_default()
        };
        (part(0), part(1), part(2))
    }

    pub fn set_delimiter(&mut self, index: u8, open: &str, escape: &str, close: &str) {
        if index >= DELIMITERS {
            return;
        }
        let mut parts: Vec<(String, String, String)> =
            (0..DELIMITERS).map(|at| self.delimiter(at)).collect();
        parts[usize::from(index)] = (
            open.trim().to_owned(),
            escape.trim().to_owned(),
            close.trim().to_owned(),
        );
        let any = parts.iter().any(|(open, _, _)| !open.is_empty());
        let coded: Vec<String> = parts
            .iter()
            .enumerate()
            .flat_map(|(at, (open, escape, close))| {
                let base = at * 3;
                [
                    format!("{base:02}{open}"),
                    format!("{:02}{escape}", base + 1),
                    format!("{:02}{close}", base + 2),
                ]
            })
            .collect();
        let value = if any { coded.join(" ") } else { String::new() };
        self.set_list("Delimiters", &value);
    }

    /// The `<UserLang>` element of this language.
    pub fn to_xml(&self) -> String {
        let mut xml = String::new();
        let attribute = |xml: &mut String, name: &str, value: &str| {
            let _ = write!(xml, " {name}=\"{}\"", escape(value));
        };
        xml.push_str("    <UserLang");
        attribute(&mut xml, "name", &self.name);
        attribute(&mut xml, "ext", &self.extensions);
        attribute(&mut xml, "udlVersion", UDL_VERSION);
        for (name, value) in &self.lang_attributes {
            attribute(&mut xml, name, value);
        }
        xml.push_str(">\n        <Settings>\n            <Global");
        attribute(
            &mut xml,
            "caseIgnored",
            if self.ignore_case { "yes" } else { "no" },
        );
        let force = match self.line_comment_at {
            LineCommentAt::Anywhere => "0",
            LineCommentAt::LineStartOrIndent => "1",
            LineCommentAt::LineStart => "2",
        };
        attribute(&mut xml, "forcePureLC", force);
        for (name, value) in &self.global_attributes {
            attribute(&mut xml, name, value);
        }
        xml.push_str(" />\n            <Prefix");
        for (index, prefix) in self.prefix.iter().enumerate() {
            attribute(
                &mut xml,
                &format!("Keywords{}", index + 1),
                if *prefix { "yes" } else { "no" },
            );
        }
        xml.push_str(" />\n        </Settings>\n        <KeywordLists>\n");
        for (name, value) in &self.lists {
            let _ = writeln!(
                xml,
                "            <Keywords name=\"{}\">{}</Keywords>",
                escape(name),
                escape(value)
            );
        }
        xml.push_str("        </KeywordLists>\n        <Styles>\n");
        for style in &self.styles {
            xml.push_str("            <WordsStyle");
            for (name, value) in style {
                attribute(&mut xml, name, value);
            }
            xml.push_str(" />\n");
        }
        xml.push_str("        </Styles>\n    </UserLang>\n");
        xml
    }
}

/// A UDL file holding `languages`.
pub fn udl_file(languages: &[UdlDefinition]) -> String {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n<NotepadPlus>\n");
    for language in languages {
        xml.push_str(&language.to_xml());
    }
    xml.push_str("</NotepadPlus>\n");
    xml
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
