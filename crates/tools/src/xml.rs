//! XML, as Notepad++'s XML Tools plugin handles it: checking that a document is well formed,
//! pretty printing it with one element per line, and putting it on one line ("linearize").
//!
//! Tags, attributes, comments, CDATA sections and the text of elements are copied as they are
//! written. Only whitespace between tags changes: an element holding text alone stays on one
//! line with its text untouched; in mixed content, text is trimmed onto lines of its own.

use roxmltree::{Document, ParsingOptions};

use crate::{Layout, SyntaxError};

const BOM: &str = "\u{feff}";

/// Checks that `text` is a well-formed XML document.
pub fn check(text: &str) -> Result<(), SyntaxError> {
    let options = ParsingOptions {
        allow_dtd: true,
        ..ParsingOptions::default()
    };
    match Document::parse_with_options(text, options) {
        Ok(_) => Ok(()),
        Err(error) => {
            let position = error.pos();
            let offset = offset_of(text, position.row as usize, position.col as usize);
            Err(SyntaxError::new(offset, message(&error)))
        }
    }
}

/// Why the document does not read: the most common reasons in the interface's language,
/// roxmltree's own words for the rest.
fn message(error: &roxmltree::Error) -> String {
    match error {
        roxmltree::Error::NoRootNode => birchpad_i18n::t!("tools.xml.no-root"),
        roxmltree::Error::UnclosedRootNode => birchpad_i18n::t!("tools.xml.root-not-closed"),
        other => other.to_string(),
    }
}

/// The byte offset of a 1-based line and column (in characters).
fn offset_of(text: &str, line: usize, column: usize) -> usize {
    let line_start = if line <= 1 {
        0
    } else {
        text.match_indices('\n')
            .nth(line - 2)
            .map_or(text.len(), |(at, _)| at + 1)
    };
    text[line_start..]
        .char_indices()
        .nth(column.saturating_sub(1))
        .map_or(text.len(), |(at, _)| line_start + at)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// `<?...?>`, `<!--...-->`, `<!DOCTYPE ...>`: a line of its own.
    Other,
    Start,
    End,
    /// `<a/>`.
    Empty,
    /// Text, or a CDATA section.
    Text,
}

#[derive(Debug, Clone, Copy)]
struct Token<'a> {
    kind: Kind,
    text: &'a str,
}

/// The tokens of a document known to be well formed.
fn tokens(text: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let rest = &text[at..];
        let (kind, len) = if !rest.starts_with('<') {
            (Kind::Text, rest.find('<').unwrap_or(rest.len()))
        } else if rest.starts_with("<!--") {
            (Kind::Other, through(rest, "-->"))
        } else if rest.starts_with("<![CDATA[") {
            (Kind::Text, through(rest, "]]>"))
        } else if rest.starts_with("<?") {
            (Kind::Other, through(rest, "?>"))
        } else if rest.starts_with("<!") {
            (Kind::Other, declaration_end(rest))
        } else {
            let len = tag_end(rest);
            let kind = if rest.starts_with("</") {
                Kind::End
            } else if rest[..len].ends_with("/>") {
                Kind::Empty
            } else {
                Kind::Start
            };
            (kind, len)
        };
        tokens.push(Token {
            kind,
            text: &rest[..len],
        });
        at += len;
    }
    tokens
}

/// The length of `rest` through the first `end`, or all of it.
fn through(rest: &str, end: &str) -> usize {
    rest.find(end).map_or(rest.len(), |at| at + end.len())
}

/// The length of a tag, whose attribute values may hold `>`.
fn tag_end(rest: &str) -> usize {
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

/// The length of `<!DOCTYPE ...>`, whose internal subset in `[...]` may hold `>`.
fn declaration_end(rest: &str) -> usize {
    let bytes = rest.as_bytes();
    let mut quote = None;
    let mut depth = 0usize;
    let mut at = 0;
    while at < bytes.len() {
        let byte = bytes[at];
        match quote {
            Some(open) if open == byte => quote = None,
            Some(_) => {}
            None => match byte {
                b'"' | b'\'' => quote = Some(byte),
                // A comment may hold anything.
                b'<' if rest[at..].starts_with("<!--") => {
                    at += through(&rest[at..], "-->");
                    continue;
                }
                b'[' => depth += 1,
                b']' => depth = depth.saturating_sub(1),
                b'>' if depth == 0 => return at + 1,
                _ => {}
            },
        }
        at += 1;
    }
    rest.len()
}

/// `text` with every element on a line of its own, indented by nesting.
pub fn pretty_print(text: &str, layout: Layout<'_>) -> Result<String, SyntaxError> {
    check(text)?;
    // A byte order mark stays first, with the XML declaration right after it.
    let (bom, text) = text
        .strip_prefix(BOM)
        .map_or(("", text), |rest| (BOM, rest));
    let tokens = tokens(text);
    let mut out = String::with_capacity(text.len() * 2);
    out.push_str(bom);
    let mut depth = 0usize;
    let line = |out: &mut String, depth: usize, piece: &str| {
        if out.len() > bom.len() {
            out.push_str(layout.line_ending);
        }
        layout.indent.write(out, depth);
        out.push_str(piece);
    };
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        match token.kind {
            Kind::Start => {
                // An element holding only text (or nothing) stays on one line.
                let texts = tokens[index + 1..]
                    .iter()
                    .take_while(|next| next.kind == Kind::Text)
                    .count();
                let close = index + 1 + texts;
                if tokens.get(close).is_some_and(|next| next.kind == Kind::End) {
                    let inner: String = tokens[index + 1..close].iter().map(|t| t.text).collect();
                    let whole = format!("{}{}{}", token.text, inner, tokens[close].text);
                    if texts == 0 || !inner.trim().is_empty() {
                        line(&mut out, depth, &whole);
                    } else {
                        line(
                            &mut out,
                            depth,
                            &format!("{}{}", token.text, tokens[close].text),
                        );
                    }
                    index = close + 1;
                    continue;
                }
                line(&mut out, depth, token.text);
                depth += 1;
            }
            Kind::End => {
                depth = depth.saturating_sub(1);
                line(&mut out, depth, token.text);
            }
            Kind::Empty | Kind::Other => line(&mut out, depth, token.text),
            Kind::Text => {
                let trimmed = token.text.trim();
                if !trimmed.is_empty() {
                    line(&mut out, depth, trimmed);
                }
            }
        }
        index += 1;
    }
    Ok(out)
}

/// `text` on one line: whitespace between tags is taken out, text is kept.
pub fn linearize(text: &str) -> Result<String, SyntaxError> {
    check(text)?;
    Ok(tokens(text)
        .into_iter()
        .filter(|token| token.kind != Kind::Text || !token.text.trim().is_empty())
        .map(|token| token.text)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Indent;

    fn pretty(text: &str) -> String {
        pretty_print(
            text,
            Layout {
                indent: Indent::Spaces(2),
                line_ending: "\n",
            },
        )
        .unwrap()
    }

    #[test]
    fn puts_elements_on_lines_of_their_own() {
        let text = r#"<?xml version="1.0"?><!-- c --><a x="1>2"><b>text</b><c/><d></d><e>  </e><f><g/></f></a>"#;
        assert_eq!(
            pretty(text),
            "<?xml version=\"1.0\"?>\n<!-- c -->\n<a x=\"1>2\">\n  <b>text</b>\n  <c/>\n  <d></d>\n  <e></e>\n  <f>\n    <g/>\n  </f>\n</a>"
        );
    }

    #[test]
    fn keeps_text_and_cdata_as_written() {
        assert_eq!(pretty("<a> x  y </a>"), "<a> x  y </a>");
        assert_eq!(pretty("<a><![CDATA[<b> ]]></a>"), "<a><![CDATA[<b> ]]></a>");
        // Mixed content: text trimmed onto its own line.
        assert_eq!(
            pretty("<a> hi <b/> there </a>"),
            "<a>\n  hi\n  <b/>\n  there\n</a>"
        );
    }

    #[test]
    fn keeps_a_doctype_with_its_internal_subset() {
        let text = "<!DOCTYPE a [<!ENTITY e \"x>y\"><!-- ] > --><!ELEMENT a ANY>]><a>&e;</a>";
        assert_eq!(
            pretty(text),
            "<!DOCTYPE a [<!ENTITY e \"x>y\"><!-- ] > --><!ELEMENT a ANY>]>\n<a>&e;</a>"
        );
    }

    #[test]
    fn reindents_what_is_already_indented() {
        let text = "<a>\r\n\t\t<b>1</b>\r\n   <b>2</b>\r\n</a>\r\n";
        let layout = Layout {
            indent: Indent::Tab,
            line_ending: "\r\n",
        };
        assert_eq!(
            pretty_print(text, layout).unwrap(),
            "<a>\r\n\t<b>1</b>\r\n\t<b>2</b>\r\n</a>"
        );
        assert_eq!(linearize(text).unwrap(), "<a><b>1</b><b>2</b></a>");
        assert_eq!(linearize(&pretty(text)).unwrap(), linearize(text).unwrap());
    }

    #[test]
    fn says_where_a_document_is_not_well_formed() {
        for (text, offset) in [
            ("<a>", 0),
            ("<a></b>", 3),
            ("<a x=1/>", 5),
            ("", 0),
            ("text", 0),
            ("<a/><b/>", 4),
            ("<a>\n  <b></a>", 9),
        ] {
            let error = check(text).expect_err(text);
            assert_eq!(error.offset, offset, "{text}: {}", error.message);
            assert!(pretty_print(text, Layout::default()).is_err());
            assert!(linearize(text).is_err());
        }
    }

    #[test]
    fn positions_count_characters() {
        assert_eq!(offset_of("мир\nab", 2, 2), 8);
        assert_eq!(offset_of("мир", 1, 2), 2);
        assert_eq!(offset_of("ab", 1, 9), 2);
        assert_eq!(offset_of("ab", 5, 1), 2);
        assert_eq!(offset_of("ab", 0, 0), 0);
    }

    #[test]
    fn a_byte_order_mark_stays_right_before_the_declaration() {
        let text = "\u{feff}<?xml version=\"1.0\"?>\n<a><b/></a>";
        let pretty_text = pretty(text);
        assert_eq!(
            pretty_text,
            "\u{feff}<?xml version=\"1.0\"?>\n<a>\n  <b/>\n</a>"
        );
        assert!(check(&pretty_text).is_ok());
        assert_eq!(pretty(&pretty_text), pretty_text);
        assert_eq!(
            linearize(text).unwrap(),
            "\u{feff}<?xml version=\"1.0\"?><a><b/></a>"
        );
    }

    #[test]
    fn a_byte_order_mark_before_the_root_alone() {
        assert_eq!(pretty("\u{feff}<a/>"), "\u{feff}<a/>");
        assert!(pretty_print("\u{feff}", Layout::default()).is_err());
        // Only at the very start is it a byte order mark.
        assert!(pretty_print("<a/>\u{feff}", Layout::default()).is_err());
    }

    #[test]
    fn a_single_empty_element() {
        assert_eq!(pretty("<a/>"), "<a/>");
        assert_eq!(linearize("  <a/>  ").unwrap(), "<a/>");
    }
}
