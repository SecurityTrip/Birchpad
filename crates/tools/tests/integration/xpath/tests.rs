use birchpad_tools::xpath::*;

const DOC: &str = r#"<?xml version="1.0"?>
<library xmlns:x="urn:x">
  <!-- books -->
  <book id="1" lang="en"><title>Rust</title><price>30</price></book>
  <book id="2"><title>Go</title><price>25.5</price><x:note>new</x:note></book>
  <shelf><book id="3"><title>C</title><price>10</price></book></shelf>
</library>"#;

fn labels(expression: &str) -> Vec<String> {
    match evaluate(DOC, expression).unwrap() {
        XPathResult::Nodes(nodes) => nodes.into_iter().map(|node| node.label).collect(),
        XPathResult::Value(value) => vec![format!("= {value}")],
    }
}

fn texts(expression: &str) -> Vec<String> {
    match evaluate(DOC, expression).unwrap() {
        XPathResult::Nodes(nodes) => nodes
            .into_iter()
            .map(|node| DOC[node.range].to_owned())
            .collect(),
        XPathResult::Value(value) => vec![value],
    }
}

#[test]
fn selects_by_paths() {
    assert_eq!(labels("/library/book/title"), ["<title>", "<title>"]);
    assert_eq!(texts("/library/book/title/text()"), ["Rust", "Go"]);
    assert_eq!(texts("//title/text()"), ["Rust", "Go", "C"]);
    assert_eq!(labels("/library/*"), ["<book>", "<book>", "<shelf>"]);
    assert_eq!(labels("/"), ["/"]);
    assert_eq!(labels("/library"), ["<library>"]);
    // Relative to the document's root, as from the start.
    assert_eq!(labels("library/shelf"), ["<shelf>"]);
    assert_eq!(texts("//shelf//title/text()"), ["C"]);
    assert_eq!(texts("//price/../title/text()"), ["Rust", "Go", "C"]);
    assert_eq!(texts("//title/./text()"), ["Rust", "Go", "C"]);
    assert_eq!(labels("//comment()"), ["<!--books-->"]);
    // A prefix is not resolved: names match by their local part.
    assert_eq!(texts("//x:note/text()"), ["new"]);
    assert_eq!(texts("//other:note/text()"), ["new"]);
}

#[test]
fn selects_attributes() {
    assert_eq!(
        labels("//book/@id"),
        ["@id=\"1\"", "@id=\"2\"", "@id=\"3\""]
    );
    assert_eq!(texts("/library/book[1]/@*"), ["id=\"1\"", "lang=\"en\""]);
    assert_eq!(labels("//book/@missing"), Vec::<String>::new());
}

#[test]
fn filters_with_predicates() {
    assert_eq!(texts("//book[@id='2']/title/text()"), ["Go"]);
    assert_eq!(texts("//book[@lang]/title/text()"), ["Rust"]);
    assert_eq!(texts("//book[price > 20]/title/text()"), ["Rust", "Go"]);
    assert_eq!(
        texts("//book[price <= 25.5 and price != 10]/@id"),
        ["id=\"2\""]
    );
    assert_eq!(
        texts("//book[title = 'C' or @id = '1']/@id"),
        ["id=\"1\"", "id=\"3\""]
    );
    assert_eq!(texts("//book[not(@lang)]/@id"), ["id=\"2\"", "id=\"3\""]);
    // Positions count among each parent's: the shelf's book is the first of its own.
    assert_eq!(texts("//book[1]/@id"), ["id=\"1\"", "id=\"3\""]);
    assert_eq!(texts("/library/book[last()]/@id"), ["id=\"2\""]);
    assert_eq!(texts("/library/book[position() = 2]/@id"), ["id=\"2\""]);
    assert_eq!(texts("//book[contains(title, 'us')]/@id"), ["id=\"1\""]);
    assert_eq!(texts("//book[starts-with(@id, '3')]/title/text()"), ["C"]);
    assert_eq!(texts("//book[string-length(title) = 2]/@id"), ["id=\"2\""]);
    assert_eq!(texts("//book[x:note][1]/@id"), ["id=\"2\""]);
    assert_eq!(texts("//*[name() = 'shelf']/book/@id"), ["id=\"3\""]);
    assert_eq!(texts("//title[normalize-space() = 'Go']/text()"), ["Go"]);
    // Nothing matches: nothing.
    assert_eq!(texts("//book[@id = '9']"), Vec::<String>::new());
    assert_eq!(texts("//book[5]"), Vec::<String>::new());
}

#[test]
fn unions_come_in_document_order_once() {
    assert_eq!(
        texts("//price/text() | //title/text() | //title/text()"),
        ["Rust", "30", "Go", "25.5", "C", "10"]
    );
}

#[test]
fn computes_values() {
    assert_eq!(labels("count(//book)"), ["= 3"]);
    assert_eq!(labels("count(//book[price > 100])"), ["= 0"]);
    assert_eq!(labels("//book[1]/price > 29"), ["= true"]);
    assert_eq!(labels("'text'"), ["= text"]);
    assert_eq!(labels("1.5"), ["= 1.5"]);
    assert_eq!(labels("-2"), ["= -2"]);
    assert_eq!(labels("name(/library/*[3])"), ["= shelf"]);
    assert_eq!(labels("local-name(//x:note)"), ["= note"]);
}

#[test]
fn refuses_what_it_cannot_read() {
    for (expression, expected) in [
        ("", "empty"),
        ("   ", "empty"),
        ("//book[", "too soon"),
        ("//book[@id='1'", "too soon"),
        ("//book[@id='1]", "quote"),
        ("//book)", "unexpected"),
        ("frobnicate(1)", "not a function"),
        ("count()", "not a function"),
        ("contains('a')", "not a function"),
        ("//.", "cannot follow"),
        ("1 | 2", "node sets"),
        ("count(1)", "node set"),
        ("/library/9book", "unexpected"),
    ] {
        let error = evaluate(DOC, expression).unwrap_err().to_string();
        assert!(error.contains(expected), "{expression}: {error}");
    }
    let nested = format!("{}1{}", "(".repeat(100), ")".repeat(100));
    let error = evaluate(DOC, &nested).unwrap_err().to_string();
    assert!(error.contains("nested"), "{error}");
    let fine = format!("{}1{}", "(".repeat(10), ")".repeat(10));
    assert!(evaluate(DOC, &fine).is_ok());
}

#[test]
fn refuses_a_document_that_is_not_xml() {
    let error = evaluate("<a><b></a>", "//b").unwrap_err().to_string();
    assert!(error.contains("not XML"), "{error}");
    assert!(evaluate("", "/").is_err());
}

#[test]
fn ranges_cover_the_nodes() {
    let XPathResult::Nodes(nodes) = evaluate(DOC, "//book[@id='3']").unwrap() else {
        panic!("nodes")
    };
    assert_eq!(
        &DOC[nodes[0].range.clone()],
        "<book id=\"3\"><title>C</title><price>10</price></book>"
    );
    // A long text is cut short in its label, not in its range.
    let long = format!("<a>{}</a>", "x ".repeat(100));
    let XPathResult::Nodes(nodes) = evaluate(&long, "/a/text()").unwrap() else {
        panic!("nodes")
    };
    assert!(nodes[0].label.ends_with('…'));
    assert_eq!(nodes[0].range.len(), 200);
}

#[test]
fn double_slash_positions_count_among_siblings() {
    // Nested elements of one name come between siblings in document order.
    let doc = r#"<a><b id="1"><b id="2"/><b id="4"/></b><b id="3"/></a>"#;
    let ids = |expression: &str| match evaluate(doc, expression).unwrap() {
        XPathResult::Nodes(nodes) => nodes
            .into_iter()
            .map(|node| doc[node.range].to_owned())
            .collect::<Vec<_>>(),
        XPathResult::Value(value) => vec![value],
    };
    assert_eq!(ids("//b[1]/@id"), ["id=\"1\"", "id=\"2\""]);
    assert_eq!(ids("//b[last()]/@id"), ["id=\"4\"", "id=\"3\""]);
    // Past every parent's last: nothing.
    assert!(ids("//b[3]/@id").is_empty());
}

#[test]
fn a_character_of_several_bytes_where_and_or_or_could_start() {
    // "o€" is checked for "or" two bytes in, in the middle of the "€".
    for expression in ["1 o€", "1 an€", "//book[o\u{feff}r]"] {
        assert!(evaluate(DOC, expression).is_err(), "{expression}");
    }
    // A name that only starts like the word stays a name.
    assert_eq!(texts("//book[order]/@id"), Vec::<String>::new());
}
