use birchpad_tools::json_tree::*;
use birchpad_tools::{Indent, Layout};

const STORE: &str = r#"{"store": {"book": [{"title": "A", "price": 8.95}, {"title": "B \"q\"", "tags": []}], "a b": {}}, "n": null, "t": true}"#;

fn layout() -> Layout<'static> {
    Layout {
        indent: Indent::Spaces(2),
        line_ending: "\n",
    }
}

#[test]
fn reads_values_with_their_keys_and_places() {
    let tree = JsonTree::parse(STORE).unwrap();
    let root = &tree.nodes[0];
    assert_eq!(
        (root.kind, root.key.clone(), root.depth),
        (JsonKind::Object, None, 0)
    );
    assert_eq!(root.range, 0..STORE.len());
    assert_eq!(root.children.len(), 3);
    let store = &tree.nodes[root.children[0]];
    assert_eq!(store.key, Some(JsonKey::Name("store".into())));
    assert_eq!(&STORE[store.key_range.clone().unwrap()], "\"store\"");
    let book = tree.find("store.book").unwrap();
    assert_eq!(tree.nodes[book].kind, JsonKind::Array);
    let second = tree.nodes[book].children[1];
    assert_eq!(tree.nodes[second].key, Some(JsonKey::Index(1)));
    assert_eq!(tree.nodes[second].key_range, None);
    let price = tree.find("$.store.book[0].price").unwrap();
    assert_eq!(&STORE[tree.nodes[price].range.clone()], "8.95");
    assert_eq!(tree.nodes[price].kind, JsonKind::Number);
    assert_eq!(tree.nodes[tree.find("n").unwrap()].kind, JsonKind::Null);
    assert_eq!(tree.nodes[tree.find("t").unwrap()].kind, JsonKind::Bool);
    let tags = tree.find("store.book[1].tags").unwrap();
    assert_eq!(tree.nodes[tags].kind, JsonKind::Array);
    assert!(tree.nodes[tags].children.is_empty());
}

#[test]
fn a_scalar_alone_is_a_tree_of_one() {
    for (text, kind) in [
        ("\"x\"", JsonKind::String),
        (" 1e3 ", JsonKind::Number),
        ("false", JsonKind::Bool),
        ("null", JsonKind::Null),
        ("[]", JsonKind::Array),
        ("{}", JsonKind::Object),
    ] {
        let tree = JsonTree::parse(text).unwrap();
        assert_eq!(tree.nodes.len(), 1, "{text}");
        assert_eq!(tree.nodes[0].kind, kind, "{text}");
        assert_eq!(tree.path(0), "$");
        assert_eq!(tree.find("$"), Ok(0));
        assert_eq!(tree.find(""), Ok(0));
    }
}

#[test]
fn refuses_what_is_not_json() {
    for text in ["", "{", "[1,]", "{\"a\" 1}", "nul", "{\"a\":1}x"] {
        assert!(JsonTree::parse(text).is_err(), "{text:?}");
        assert!(sort_keys(text, layout()).is_err(), "{text:?}");
    }
}

#[test]
fn paths_name_values_and_find_them_again() {
    let tree = JsonTree::parse(STORE).unwrap();
    let title = tree.find("store.book[1].title").unwrap();
    assert_eq!(tree.path(title), "$.store.book[1].title");
    let spaced = tree.find("$.store[\"a b\"]").unwrap();
    assert_eq!(tree.path(spaced), "$.store[\"a b\"]");
    assert_eq!(tree.find("$['store']['a b']"), Ok(spaced));
    assert_eq!(tree.find(" $.store.book[ 0 ] "), tree.find("store.book[0]"));
    for index in 0..tree.nodes.len() {
        assert_eq!(tree.find(&tree.path(index)), Ok(index));
    }
}

#[test]
fn says_what_is_wrong_with_a_path() {
    let tree = JsonTree::parse(STORE).unwrap();
    for (path, expected) in [
        ("store.missing", "no value"),
        ("store.book[5]", "no value"),
        ("store.book.title", "no value"),
        ("store..book", "missing after a dot"),
        ("store.book[x]", "not an index"),
        ("store.book[-1]", "not an index"),
        ("store.book[0", "expected ]"),
        ("store[\"a b", "not closed"),
        ("store[\"a b\"x", "expected ]"),
        ("store.book[0]x", "unexpected"),
    ] {
        let error = tree.find(path).unwrap_err().to_string();
        assert!(error.contains(expected), "{path}: {error}");
    }
}

#[test]
fn copies_strings_unescaped_and_others_as_written() {
    let text = r#"{"s": "a\"b\\c\n\u00e9\ud83d\ude00", "n": -1.5e3, "o": {"x": 1}}"#;
    let tree = JsonTree::parse(text).unwrap();
    assert_eq!(tree.value(tree.find("s").unwrap(), text), "a\"b\\c\né😀");
    assert_eq!(tree.value(tree.find("n").unwrap(), text), "-1.5e3");
    assert_eq!(tree.value(tree.find("o").unwrap(), text), "{\"x\": 1}");
    // Keys are unescaped too.
    let escaped = JsonTree::parse(r#"{"a\u0020b": 1}"#).unwrap();
    assert_eq!(escaped.find("[\"a b\"]"), Ok(1));
    // A lone surrogate becomes the replacement character.
    assert_eq!(unescape("\"\\ud83d\""), "\u{fffd}");
    assert_eq!(unescape("\"\""), "");
}

#[test]
fn sorts_the_keys_of_every_object() {
    let text = r#"{"b": {"z": 1, "a": [3, {"d": 0, "c": 1}]}, "a": "x", "b2": {}}"#;
    let sorted = sort_keys(text, layout()).unwrap();
    assert_eq!(
        sorted,
        "{\n  \"a\": \"x\",\n  \"b\": {\n    \"a\": [\n      3,\n      {\n        \"c\": 1,\n        \"d\": 0\n      }\n    ],\n    \"z\": 1\n  },\n  \"b2\": {}\n}"
    );
    // Sorting again changes nothing; arrays keep their order; same names keep theirs.
    assert_eq!(sort_keys(&sorted, layout()).unwrap(), sorted);
    assert_eq!(
        sort_keys(r#"{"k": 2, "k": 1}"#, layout()).unwrap(),
        "{\n  \"k\": 2,\n  \"k\": 1\n}"
    );
    assert_eq!(sort_keys("[]", layout()).unwrap(), "[]");
    assert_eq!(sort_keys("7", layout()).unwrap(), "7");
}

#[test]
fn deep_nesting_neither_builds_nor_drops_recursively() {
    let depth = birchpad_tools::json::MAX_DEPTH;
    let text = format!("{}{}", "[".repeat(depth), "]".repeat(depth));
    let tree = JsonTree::parse(&text).unwrap();
    assert_eq!(tree.nodes.len(), depth);
    assert_eq!(tree.nodes[depth - 1].depth, depth - 1);
    // Without indentation: indented, the text would grow with the square of the depth.
    let flat = Layout {
        indent: Indent::Spaces(0),
        line_ending: "\n",
    };
    let sorted = sort_keys(&text, flat).unwrap();
    assert_eq!(JsonTree::parse(&sorted).unwrap().nodes.len(), depth);
    drop(tree);
    let deeper = format!("{}{}", "[".repeat(depth + 1), "]".repeat(depth + 1));
    assert!(JsonTree::parse(&deeper).is_err());
}

#[test]
fn a_key_named_twice_still_finds_every_value() {
    // The second "a" holds what the path names; the first does not.
    let text = r#"{"a": 1, "a": {"b": [true, {"a": 2}]}, "c": {"a": null}}"#;
    let tree = JsonTree::parse(text).unwrap();
    let b = tree.find("$.a.b").unwrap();
    assert_eq!(&text[tree.nodes[b].range.clone()], r#"[true, {"a": 2}]"#);
    assert_eq!(tree.value(tree.find("$.a.b[1].a").unwrap(), text), "2");
    // The first of the name, where both would do.
    assert_eq!(tree.value(tree.find("$.a").unwrap(), text), "1");
    for index in 0..tree.nodes.len() {
        let found = tree.find(&tree.path(index)).unwrap();
        assert_eq!(tree.path(found), tree.path(index));
    }
    // Nowhere under either.
    assert!(tree.find("$.a.x").is_err());
    assert!(tree.find("$.a.b[2]").is_err());
}

#[test]
fn values_lose_every_escape() {
    let text = r#"{"s": "a\r\b\f\n\t\"\\\/é"}"#;
    let tree = JsonTree::parse(text).unwrap();
    let at = tree.find("$.s").unwrap();
    assert_eq!(tree.value(at, text), "a\r\u{8}\u{c}\n\t\"\\/é");
}
