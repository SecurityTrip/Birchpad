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
#[path = "../tests/unit/xml/tests.rs"]
mod tests;
