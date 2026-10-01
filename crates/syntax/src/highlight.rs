//! Highlight names: the vocabulary between syntax queries and themes.
//!
//! Queries capture nodes with names like `@function.method.call`; themes style a fixed list of
//! names. A capture resolves to the longest name in [`HIGHLIGHT_NAMES`] that is a dot-separated
//! prefix of it (`function.method`), once when the query is compiled.

/// An index into [`HIGHLIGHT_NAMES`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Highlight(pub u16);

impl Highlight {
    pub fn name(self) -> &'static str {
        HIGHLIGHT_NAMES[usize::from(self.0)]
    }

    /// The highlight with this exact name.
    pub fn named(name: &str) -> Option<Self> {
        HIGHLIGHT_NAMES
            .iter()
            .position(|known| *known == name)
            .map(|index| Self(index as u16))
    }
}

/// Every name a theme can style. A theme falls back from `function.method` to `function`.
pub const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",
    "boolean",
    "character",
    "comment",
    "comment.documentation",
    "constant",
    "constant.builtin",
    "constructor",
    "embedded",
    "error",
    "escape",
    "function",
    "function.builtin",
    "function.macro",
    "function.method",
    "keyword",
    "keyword.directive",
    "keyword.operator",
    "label",
    "module",
    "number",
    "operator",
    "property",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "punctuation.special",
    "string",
    "string.escape",
    "string.regexp",
    "string.special",
    "string.special.key",
    "tag",
    "tag.attribute",
    "type",
    "type.builtin",
    "variable",
    "variable.builtin",
    "variable.member",
    "variable.parameter",
    // Markup (Markdown) and diffs.
    "markup.heading",
    "markup.italic",
    "markup.strong",
    "markup.link",
    "markup.link.url",
    "markup.raw",
    "markup.list",
    "markup.quote",
    "diff.plus",
    "diff.minus",
    "diff.delta",
    // Synonyms some grammars use, mapped by `resolve` below.
];

/// Capture names some grammars use for things in the list above.
const SYNONYMS: &[(&str, &str)] = &[
    ("text.title", "markup.heading"),
    ("text.emphasis", "markup.italic"),
    ("text.strong", "markup.strong"),
    ("text.literal", "markup.raw"),
    ("text.uri", "markup.link.url"),
    ("text.reference", "markup.link"),
    ("text.quote", "markup.quote"),
    ("markup.bold", "markup.strong"),
    ("addition", "diff.plus"),
    ("deletion", "diff.minus"),
    ("attribute.builtin", "attribute"),
    ("include", "keyword.directive"),
    ("preproc", "keyword.directive"),
    ("define", "keyword.directive"),
    ("conditional", "keyword"),
    ("repeat", "keyword"),
    ("exception", "keyword"),
    ("storageclass", "keyword"),
    ("namespace", "module"),
    ("field", "property"),
    ("parameter", "variable.parameter"),
    ("float", "number"),
    ("delimiter", "punctuation.delimiter"),
];

/// The highlight for a capture name, or `None` if themes have nothing for it (captures such
/// as `@spell` or `@none`).
pub fn resolve(capture: &str) -> Option<Highlight> {
    let mut name = capture;
    loop {
        if let Some(found) = Highlight::named(name) {
            return Some(found);
        }
        if let Some(&(_, target)) = SYNONYMS.iter().find(|(synonym, _)| *synonym == name) {
            return Highlight::named(target);
        }
        name = &name[..name.rfind('.')?];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_by_longest_prefix() {
        let name = |capture| resolve(capture).map(Highlight::name);
        assert_eq!(name("function.method.call"), Some("function.method"));
        assert_eq!(name("function.call"), Some("function"));
        assert_eq!(name("keyword"), Some("keyword"));
        assert_eq!(name("text.title"), Some("markup.heading"));
        assert_eq!(name("float"), Some("number"));
        assert_eq!(name("spell"), None);
        assert_eq!(name("none"), None);
    }
}
