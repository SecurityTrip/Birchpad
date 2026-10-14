//! JSON, as Notepad++'s JSON Viewer plugin handles it: checking that a text is JSON (RFC 8259),
//! formatting it with one value per line, and putting it on one line.
//!
//! Strings and numbers are copied as they are written, so formatting never changes a value:
//! `1.0` stays `1.0` and `"\u00e9"` stays escaped. Nesting is followed with a stack of its
//! own, so a deeply nested text cannot overflow the call stack.

use birchpad_i18n::t;

use crate::{Layout, SyntaxError};

/// The deepest nesting accepted; deeper texts are refused rather than formatted.
pub const MAX_DEPTH: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    OpenObject,
    CloseObject,
    OpenArray,
    CloseArray,
    Colon,
    Comma,
    /// A string, number, `true`, `false` or `null`.
    Scalar,
    String,
}

#[derive(Debug, Clone, Copy)]
struct Token {
    kind: Kind,
    start: usize,
    end: usize,
}

/// Checks that `text` is one JSON value, with only whitespace around it.
pub fn validate(text: &str) -> Result<(), SyntaxError> {
    walk(text, &mut |_, _| {})
}

/// `text` with one value or member per line, indented by nesting; empty objects and arrays
/// stay `{}` and `[]`.
pub fn format(text: &str, layout: Layout<'_>) -> Result<String, SyntaxError> {
    let mut out = String::with_capacity(text.len() * 2);
    let mut depth = 0usize;
    let mut previous: Option<Kind> = None;
    walk(text, &mut |token, value| {
        let opens = matches!(previous, Some(Kind::OpenObject | Kind::OpenArray));
        match token.kind {
            Kind::CloseObject | Kind::CloseArray => {
                depth -= 1;
                if !opens {
                    out.push_str(layout.line_ending);
                    layout.indent.write(&mut out, depth);
                }
                out.push_str(value);
            }
            _ => {
                if opens || previous == Some(Kind::Comma) {
                    out.push_str(layout.line_ending);
                    layout.indent.write(&mut out, depth);
                }
                out.push_str(value);
                match token.kind {
                    Kind::Colon => out.push(' '),
                    Kind::OpenObject | Kind::OpenArray => depth += 1,
                    _ => {}
                }
            }
        }
        previous = Some(token.kind);
    })?;
    Ok(out)
}

/// `text` on one line, without any whitespace between tokens.
pub fn minify(text: &str) -> Result<String, SyntaxError> {
    let mut out = String::with_capacity(text.len());
    walk(text, &mut |_, value| out.push_str(value))?;
    Ok(out)
}

/// Reads `text` and hands every token to `emit` in order, once the whole text has been read
/// and found to be JSON. Nothing is emitted for a text that is not.
fn walk(text: &str, emit: &mut dyn FnMut(Token, &str)) -> Result<(), SyntaxError> {
    let tokens = lex(text)?;
    check(text, &tokens)?;
    for token in tokens {
        emit(token, &text[token.start..token.end]);
    }
    Ok(())
}

fn lex(text: &str) -> Result<Vec<Token>, SyntaxError> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let start = at;
        let kind = match bytes[at] {
            b' ' | b'\t' | b'\n' | b'\r' => {
                at += 1;
                continue;
            }
            b'{' => Kind::OpenObject,
            b'}' => Kind::CloseObject,
            b'[' => Kind::OpenArray,
            b']' => Kind::CloseArray,
            b':' => Kind::Colon,
            b',' => Kind::Comma,
            b'"' => {
                at = string_end(text, at)?;
                tokens.push(Token {
                    kind: Kind::String,
                    start,
                    end: at,
                });
                continue;
            }
            b'-' | b'0'..=b'9' => {
                at = number_end(bytes, at)?;
                tokens.push(Token {
                    kind: Kind::Scalar,
                    start,
                    end: at,
                });
                continue;
            }
            b't' | b'f' | b'n' => {
                let word = ["true", "false", "null"]
                    .into_iter()
                    .find(|word| text[at..].starts_with(word))
                    .ok_or_else(|| unexpected(text, at))?;
                at += word.len();
                if bytes.get(at).is_some_and(u8::is_ascii_alphanumeric) {
                    return Err(unexpected(text, at));
                }
                tokens.push(Token {
                    kind: Kind::Scalar,
                    start,
                    end: at,
                });
                continue;
            }
            _ => return Err(unexpected(text, at)),
        };
        at += 1;
        tokens.push(Token {
            kind,
            start,
            end: at,
        });
    }
    Ok(tokens)
}

fn unexpected(text: &str, at: usize) -> SyntaxError {
    match text[at..].chars().next() {
        Some(found) => SyntaxError::new(
            at,
            t!("tools.json.unexpected", found = format!("{found:?}")),
        ),
        None => SyntaxError::new(at, t!("tools.json.unexpected-end")),
    }
}

/// The end of the string that starts with the quote at `start`.
fn string_end(text: &str, start: usize) -> Result<usize, SyntaxError> {
    let bytes = text.as_bytes();
    let mut at = start + 1;
    loop {
        match bytes.get(at) {
            None => return Err(SyntaxError::new(start, t!("tools.json.string-not-closed"))),
            Some(b'"') => return Ok(at + 1),
            Some(b'\\') => match bytes.get(at + 1) {
                Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => at += 2,
                Some(b'u') => {
                    let digits = bytes.get(at + 2..at + 6);
                    if !digits.is_some_and(|digits| digits.iter().all(u8::is_ascii_hexdigit)) {
                        return Err(SyntaxError::new(at, t!("tools.json.u-takes-four-digits")));
                    }
                    at += 6;
                }
                _ => return Err(SyntaxError::new(at, t!("tools.json.unknown-escape"))),
            },
            Some(byte) if *byte < 0x20 => {
                return Err(SyntaxError::new(at, t!("tools.json.control-character")));
            }
            Some(_) => at += 1,
        }
    }
}

/// The end of the number that starts at `start`: `-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?`.
fn number_end(bytes: &[u8], start: usize) -> Result<usize, SyntaxError> {
    let digits = |at: usize| {
        bytes[at.min(bytes.len())..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count()
    };
    let mut at = start;
    if bytes[at] == b'-' {
        at += 1;
    }
    match digits(at) {
        0 => return Err(SyntaxError::new(at, t!("tools.json.number-needs-digits"))),
        count if bytes[at] == b'0' && count > 1 => {
            return Err(SyntaxError::new(at, t!("tools.json.number-starts-with-0")));
        }
        count => at += count,
    }
    if bytes.get(at) == Some(&b'.') {
        at += 1;
        match digits(at) {
            0 => {
                return Err(SyntaxError::new(
                    at,
                    t!("tools.json.decimal-point-needs-digits"),
                ));
            }
            count => at += count,
        }
    }
    if matches!(bytes.get(at), Some(b'e' | b'E')) {
        at += 1;
        if matches!(bytes.get(at), Some(b'+' | b'-')) {
            at += 1;
        }
        match digits(at) {
            0 => return Err(SyntaxError::new(at, t!("tools.json.exponent-needs-digits"))),
            count => at += count,
        }
    }
    Ok(at)
}

/// What may come next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expect {
    /// A value (at the start, after a colon or a comma in an array).
    Value,
    /// A value or `]`, just after `[`.
    ValueOrClose,
    /// A key or `}`, just after `{`.
    KeyOrClose,
    /// A key, after a comma in an object.
    Key,
    Colon,
    /// A comma or the end of the innermost object or array.
    CommaOrClose,
    /// Nothing: the value is complete.
    End,
}

fn check(text: &str, tokens: &[Token]) -> Result<(), SyntaxError> {
    // `true` for an object, `false` for an array.
    let mut stack: Vec<bool> = Vec::new();
    let mut expect = Expect::Value;
    for token in tokens {
        let wrong = || unexpected(text, token.start);
        let after_value = |stack: &Vec<bool>| {
            if stack.is_empty() {
                Expect::End
            } else {
                Expect::CommaOrClose
            }
        };
        expect = match (expect, token.kind) {
            (Expect::Value | Expect::ValueOrClose, Kind::OpenObject | Kind::OpenArray) => {
                if stack.len() == MAX_DEPTH {
                    return Err(SyntaxError::new(
                        token.start,
                        t!("tools.json.nested-too-deep", levels = MAX_DEPTH),
                    ));
                }
                let object = token.kind == Kind::OpenObject;
                stack.push(object);
                if object {
                    Expect::KeyOrClose
                } else {
                    Expect::ValueOrClose
                }
            }
            (Expect::Value | Expect::ValueOrClose, Kind::Scalar | Kind::String) => {
                after_value(&stack)
            }
            (Expect::KeyOrClose | Expect::Key, Kind::String) => Expect::Colon,
            (Expect::Colon, Kind::Colon) => Expect::Value,
            (Expect::CommaOrClose, Kind::Comma) => {
                if stack.last() == Some(&true) {
                    Expect::Key
                } else {
                    Expect::Value
                }
            }
            (Expect::CommaOrClose | Expect::KeyOrClose, Kind::CloseObject)
                if stack.last() == Some(&true) =>
            {
                stack.pop();
                after_value(&stack)
            }
            (Expect::CommaOrClose | Expect::ValueOrClose, Kind::CloseArray)
                if stack.last() == Some(&false) =>
            {
                stack.pop();
                after_value(&stack)
            }
            (Expect::Key, _) | (Expect::KeyOrClose, _) => {
                return Err(SyntaxError::new(token.start, t!("tools.json.expected-key")));
            }
            _ => return Err(wrong()),
        };
    }
    match expect {
        Expect::End => Ok(()),
        _ if tokens.is_empty() => Err(SyntaxError::new(0, t!("tools.json.empty"))),
        _ => Err(SyntaxError::new(text.len(), t!("tools.json.not-closed"))),
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::Indent;

    fn pretty(text: &str) -> String {
        format(text, Layout::default()).unwrap()
    }

    #[test]
    fn formats_nested_values() {
        let text = r#"{"a":[1,2,{"b":null}],"c":{},"d":[],"e":"x"}"#;
        assert_eq!(
            pretty(text),
            "{\n    \"a\": [\n        1,\n        2,\n        {\n            \"b\": null\n        }\n    ],\n    \"c\": {},\n    \"d\": [],\n    \"e\": \"x\"\n}"
        );
        let layout = Layout {
            indent: Indent::Tab,
            line_ending: "\r\n",
        };
        assert_eq!(
            format("[ true ,false ]", layout).unwrap(),
            "[\r\n\ttrue,\r\n\tfalse\r\n]"
        );
    }

    #[test]
    fn keeps_numbers_and_strings_as_written() {
        let text = r#"[1.0, -0, 1E+2, 2e-3, "\u00e9\n\"", "мир"]"#;
        assert_eq!(
            minify(text).unwrap(),
            r#"[1.0,-0,1E+2,2e-3,"\u00e9\n\"","мир"]"#
        );
    }

    #[test]
    fn minifies_and_reformats_to_the_same() {
        let text = "{\n  \"a\" : [ 1 , { } ] ,\r\n\t\"b\": \"x y\"\n}\n";
        let small = minify(text).unwrap();
        assert_eq!(small, r#"{"a":[1,{}],"b":"x y"}"#);
        assert_eq!(minify(&pretty(text)).unwrap(), small);
    }

    #[test]
    fn single_values_are_json() {
        for text in ["0", "\"\"", "null", " true ", "-1.5e10", "[]", "{}"] {
            assert!(validate(text).is_ok(), "{text}");
            assert_eq!(pretty(text), text.trim());
        }
    }

    #[test]
    fn says_where_a_text_is_not_json() {
        for (text, offset) in [
            ("", 0),
            ("   ", 0),
            ("{", 1),
            ("[1,]", 3),
            ("[1 2]", 3),
            ("{\"a\" 1}", 5),
            ("{\"a\":1,}", 7),
            ("{a:1}", 1),
            ("[1}", 2),
            ("{\"a\":1]", 6),
            ("1 2", 2),
            ("01", 0),
            ("-", 1),
            ("1.", 2),
            ("1e", 2),
            ("1e+", 3),
            ("tru", 0),
            ("truex", 4),
            ("nul", 0),
            ("\"abc", 0),
            ("\"\\x\"", 1),
            ("\"\\u12\"", 1),
            ("\"a\tb\"", 2),
            ("[1]]", 3),
            ("'a'", 0),
            ("[,1]", 1),
            ("{,}", 1),
            ("}", 0),
            (":", 0),
        ] {
            let error = validate(text).expect_err(text);
            assert_eq!(error.offset, offset, "{text}: {}", error.message);
            assert!(format(text, Layout::default()).is_err());
            assert!(minify(text).is_err());
        }
    }

    #[test]
    fn says_what_is_wrong() {
        assert_eq!(
            validate("{1:2}").unwrap_err().message,
            "expected a key in quotes"
        );
        assert_eq!(validate("").unwrap_err().message, "the text is empty");
        assert_eq!(validate("@").unwrap_err().message, "unexpected '@'");
        assert!(validate("[1").unwrap_err().message.contains("not closed"));
    }

    #[test]
    fn refuses_nesting_past_the_limit() {
        let at_limit = "[".repeat(MAX_DEPTH) + &"]".repeat(MAX_DEPTH);
        assert!(validate(&at_limit).is_ok());
        let past = "[".repeat(MAX_DEPTH + 1) + &"]".repeat(MAX_DEPTH + 1);
        let error = validate(&past).unwrap_err();
        assert_eq!(error.offset, MAX_DEPTH);
    }

    #[test]
    fn a_byte_order_mark_or_comment_is_not_json() {
        assert!(validate("\u{feff}{}").is_err());
        assert!(validate("// x\n{}").is_err());
    }

    proptest! {
        #[test]
        fn never_panics_and_formats_what_validates(text in "[\\[\\]{}:,\"0-9a-z.eE+\\- \n\\\\]{0,40}") {
            if validate(&text).is_ok() {
                let small = minify(&text).unwrap();
                prop_assert_eq!(minify(&pretty(&text)).unwrap(), small.clone());
                prop_assert_eq!(minify(&small).unwrap(), small);
            } else {
                prop_assert!(minify(&text).is_err());
            }
        }
    }
}
