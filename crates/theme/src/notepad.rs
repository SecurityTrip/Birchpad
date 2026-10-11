//! Reading Notepad++ themes: `stylers.xml` and the files of its `themes` folder.
//!
//! A Notepad++ theme has global styles (`<GlobalStyles><WidgetStyle name="Caret colour" ...>`),
//! which become editor colors, and styles for each of Scintilla's lexers
//! (`<LexerStyles><LexerType name="cpp"><WordsStyle name="INSTRUCTION WORD" ...>`), which are
//! named after what Scintilla's lexer recognizes rather than after tree-sitter's highlight names.
//! Each lexer style is matched to a highlight name by its name ([`highlight_of`]). The style
//! most lexers give a highlight becomes the theme's style for it, and a lexer that styles it
//! otherwise gets a style of its own for its language.

use std::collections::BTreeMap;

use crate::{Color, PartialEditorColors, Style, SyntaxStyles, ThemeError, ThemeFile};

/// Converts the text of a Notepad++ theme into a theme file.
pub(crate) fn import(xml: &str) -> Result<ThemeFile, ThemeError> {
    let document =
        roxmltree::Document::parse(xml).map_err(|error| ThemeError::Xml(error.to_string()))?;
    let root = document.root_element();
    if !root.has_tag_name("NotepadPlus") {
        return Err(ThemeError::Xml(format!(
            "the root element is <{}>, not <NotepadPlus>",
            root.tag_name().name()
        )));
    }
    let child = |tag: &str| root.children().find(|node| node.has_tag_name(tag));
    let globals = child("GlobalStyles");
    let lexers = child("LexerStyles");
    if globals.is_none() && lexers.is_none() {
        return Err(ThemeError::Xml(
            "neither <GlobalStyles> nor <LexerStyles> in the file".into(),
        ));
    }

    let widget = |name: &str| {
        globals.and_then(|globals| {
            globals.children().find(|node| {
                node.has_tag_name("WidgetStyle") && node.attribute("name") == Some(name)
            })
        })
    };
    let foreground = |name: &str| widget(name).and_then(|node| color(node.attribute("fgColor")));
    let background = |name: &str| widget(name).and_then(|node| color(node.attribute("bgColor")));

    let default_background = background("Default Style");
    let mut file = ThemeFile {
        dark: None,
        editor: editor_colors(&foreground, &background),
        ..ThemeFile::default()
    };
    if let Some(lexers) = lexers {
        let lexers: Vec<_> = lexers
            .children()
            .filter(|node| node.has_tag_name("LexerType"))
            .filter_map(|node| Some((node.attribute("name")?.to_ascii_lowercase(), node)))
            .collect();
        // Notepad++ 8 styles stand-alone JavaScript as `javascript.js`; `javascript` is then
        // only JavaScript inside HTML. Older themes have `javascript` alone.
        let has_js = lexers.iter().any(|(name, _)| name == "javascript.js");
        let mut by_language: Vec<(String, SyntaxStyles)> = Vec::new();
        for (name, node) in &lexers {
            let language = match name.as_str() {
                "javascript.js" => "javascript",
                "javascript" if has_js => continue,
                // Notepad++'s own panels, not a language.
                "searchresult" => continue,
                name => name,
            };
            let mut styles = SyntaxStyles::new();
            for words in node
                .children()
                .filter(|node| node.has_tag_name("WordsStyle"))
            {
                let Some(highlight) = words
                    .attribute("name")
                    .and_then(|style| highlight_of(language, style))
                else {
                    continue;
                };
                // The first style of a lexer that maps to a highlight is the one it shows most:
                // `COMMENT` before `COMMENT LINE` and `COMMENT DOC`.
                styles
                    .entry(highlight.to_owned())
                    .or_insert_with(|| style_of(words, default_background));
            }
            if !styles.is_empty() {
                by_language.push((language.to_owned(), styles));
            }
        }
        (file.syntax, file.language) = common_styles(by_language);
    }
    Ok(file)
}

/// The global styles that are editor colors. Translucent ones keep the opacity of Birchpad's own
/// (Notepad++ draws them translucent too); the rest are opaque.
fn editor_colors(
    foreground: &dyn Fn(&str) -> Option<Color>,
    background: &dyn Fn(&str) -> Option<Color>,
) -> PartialEditorColors {
    let translucent = |color: Option<Color>, alpha: u8| color.map(|color| color.with_alpha(alpha));
    let fold = foreground("Fold");
    PartialEditorColors {
        text: foreground("Default Style"),
        background: background("Default Style"),
        selection: background("Selected text colour"),
        caret: foreground("Caret colour"),
        current_line: background("Current line background colour"),
        gutter_background: background("Line number margin"),
        gutter_text: foreground("Line number margin"),
        gutter_border: None,
        bookmark: None,
        change_modified: background("Change History modified"),
        change_saved: background("Change History saved"),
        fold_mark: fold,
        fold_line: fold,
        fold_underline: fold,
        whitespace: foreground("White space symbol"),
        eol_box: None,
        eol_text: foreground("EOL custom color"),
        indent_guide: foreground("Indent guideline style"),
        edge: foreground("Edge colour"),
        brace_match: foreground("Brace highlight style"),
        brace_bad: foreground("Bad brace colour"),
        smart_highlight: translucent(background("Smart Highlighting"), 0x64),
        find_mark: translucent(background("Find Mark Style"), 0x55),
        incremental_highlight: translucent(background("Incremental highlight all"), 0x55),
        mark_1: translucent(background("Mark Style 1"), 0x66),
        mark_2: translucent(background("Mark Style 2"), 0x66),
        mark_3: translucent(background("Mark Style 3"), 0x99),
        mark_4: translucent(background("Mark Style 4"), 0x55),
        mark_5: translucent(background("Mark Style 5"), 0x66),
        scrollbar_thumb: None,
    }
}

/// A Notepad++ color, `RRGGBB` without a `#`.
fn color(value: Option<&str>) -> Option<Color> {
    let value = value?.trim();
    if value.len() != 6 {
        return None;
    }
    format!("#{value}").parse().ok()
}

/// The style of a `<WordsStyle>`: its foreground, its background where it differs from the
/// theme's, and `fontStyle` (1 bold, 2 italic, 4 underline).
fn style_of(node: roxmltree::Node, default_background: Option<Color>) -> Style {
    let font_style: u32 = node
        .attribute("fontStyle")
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0);
    Style {
        color: color(node.attribute("fgColor")),
        background: color(node.attribute("bgColor"))
            .filter(|&color| Some(color) != default_background),
        bold: font_style & 1 != 0,
        italic: font_style & 2 != 0,
        underline: font_style & 4 != 0,
    }
}

/// The style most languages give each highlight, and for each language the styles that differ
/// from those. Ties go to the language listed first.
fn common_styles(
    by_language: Vec<(String, SyntaxStyles)>,
) -> (SyntaxStyles, BTreeMap<String, SyntaxStyles>) {
    // For each highlight, each style seen and how many languages use it, in order seen.
    let mut counts: BTreeMap<&str, Vec<(Style, usize)>> = BTreeMap::new();
    for (_, styles) in &by_language {
        for (highlight, style) in styles {
            let seen = counts.entry(highlight).or_default();
            match seen.iter_mut().find(|(known, _)| known == style) {
                Some((_, count)) => *count += 1,
                None => seen.push((*style, 1)),
            }
        }
    }
    let common: SyntaxStyles = counts
        .into_iter()
        .map(|(highlight, seen)| {
            let best = seen.iter().map(|(_, count)| *count).max().unwrap_or(0);
            let (style, _) = seen
                .into_iter()
                .find(|(_, count)| *count == best)
                .expect("a highlight is seen at least once");
            (highlight.to_owned(), style)
        })
        .collect();
    let mut languages = BTreeMap::new();
    for (language, styles) in by_language {
        let own: SyntaxStyles = styles
            .into_iter()
            .filter(|(highlight, style)| common.get(highlight) != Some(style))
            .collect();
        if !own.is_empty() {
            languages
                .entry(language)
                .or_insert_with(SyntaxStyles::new)
                .extend(own);
        }
    }
    (common, languages)
}

/// The highlight name for the Notepad++ style `name` of `lexer`'s, if any.
///
/// Scintilla's lexers name their styles each in their own way, but mostly with the same words:
/// a style whose name says COMMENT is a comment, one that says NUMBER a number. The exceptions
/// are listed first.
pub(crate) fn highlight_of(lexer: &str, name: &str) -> Option<&'static str> {
    let name = name.trim().to_ascii_uppercase();
    let exact = match (lexer, name.as_str()) {
        (_, "DEFAULT" | "IDENTIFIER" | "WHITE SPACE" | "WHITESPACE") if lexer != "yaml" => {
            return None;
        }
        ("yaml", "IDENTIFIER") => Some("string.special.key"),
        ("yaml", "REFERENCE") => Some("label"),
        ("yaml", "DOCUMENT") => Some("punctuation.special"),
        ("yaml" | "json", "KEYWORD") => Some("constant.builtin"),
        ("json", "PROPERTY NAME" | "PROPERTYNAME") => Some("string.special.key"),
        ("ini" | "props" | "toml", "SECTION") => Some("type"),
        ("ini" | "props" | "toml", "KEY") => Some("property"),
        ("ini" | "props", "ASSIGNMENT") => Some("operator"),
        ("ini" | "props", "DEFVAL" | "VALUE") => Some("string"),
        ("diff", "ADDED") => Some("diff.plus"),
        ("diff", "DELETED") => Some("diff.minus"),
        ("diff", "POSITION") => Some("diff.delta"),
        ("diff", "HEADER" | "COMMAND") => Some("keyword"),
        ("markdown", "STRONG" | "STRONG 2") => Some("markup.strong"),
        ("markdown", "EMPHASIS" | "EMPHASIS 2" | "EM1" | "EM2") => Some("markup.italic"),
        ("markdown", "BLOCKQUOTE" | "BLOCK QUOTE") => Some("markup.quote"),
        ("markdown", "LINK") => Some("markup.link"),
        ("markdown", "CODE" | "CODE2" | "CODE 2" | "CODEBK" | "CODE BLOCK") => Some("markup.raw"),
        ("html" | "xml" | "asp" | "php", "ATTRIBUTE" | "ATTRIBUTEUNKNOWN") => Some("tag.attribute"),
        ("html" | "xml" | "asp" | "php", "ENTITY") => Some("string.escape"),
        ("html" | "xml" | "asp" | "php", "XMLSTART" | "XMLEND") => Some("keyword.directive"),
        ("html" | "xml" | "asp" | "php", "CDATA") => Some("markup.raw"),
        ("css", "CLASS" | "ID" | "PSEUDOCLASS" | "PSEUDOELEMENT") => Some("attribute"),
        ("css", "VALUE") => Some("string"),
        ("css", "IMPORTANT") => Some("keyword"),
        ("css", name) if name.contains("PROPERT") => Some("property"),
        ("python", "CLASS NAME") => Some("type"),
        ("python", "DEF NAME") => Some("function"),
        ("python", "BUILTINS") => Some("function.builtin"),
        ("batch", "COMMAND") => Some("function"),
        ("batch", "VARIABLE") => Some("variable.builtin"),
        ("rust", "LIFETIME") => Some("label"),
        _ => None,
    };
    if exact.is_some() {
        return exact;
    }
    let has = |word: &str| name.contains(word);
    if has("COMMENT") {
        return Some(if has("DOC") {
            "comment.documentation"
        } else {
            "comment"
        });
    }
    let found = if has("REGEX") {
        "string.regexp"
    } else if has("ESCAPE") {
        "string.escape"
    } else if has("CHARACTER") || name == "CHAR" {
        "character"
    } else if has("STRING") || has("VERBATIM") || has("TRIPLE") || has("HEREDOC") || has("BACKTICK")
    {
        "string"
    } else if has("NUMBER") {
        "number"
    } else if has("OPERATOR") {
        "operator"
    } else if has("PREPROCESSOR") || has("DIRECTIVE") || has("PRAGMA") {
        "keyword.directive"
    } else if has("MACRO") {
        "function.macro"
    } else if has("TYPE") || has("CLASS") {
        "type"
    } else if has("FUNCTION") || has("METHOD") || has("DEFNAME") || has("DEF NAME") {
        "function"
    } else if has("BUILTIN") {
        "function.builtin"
    } else if has("DECORATOR") || has("ANNOTATION") || has("ATTRIBUTE") {
        "attribute"
    } else if has("LABEL") {
        "label"
    } else if has("HEADER") || has("HEADING") {
        "markup.heading"
    } else if has("LIST") {
        "markup.list"
    } else if has("TAG") {
        "tag"
    } else if has("INSTRUCTION") || has("KEYWORD") || has("WORD") || has("STATEMENT") {
        "keyword"
    } else {
        return None;
    };
    Some(found)
}

#[cfg(test)]
mod tests;
