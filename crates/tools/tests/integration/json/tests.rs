use proptest::prelude::*;

use birchpad_tools::Indent;
use birchpad_tools::Layout;
use birchpad_tools::json::*;

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
