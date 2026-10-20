use birchpad_tools::Indent;
use birchpad_tools::Layout;
use birchpad_tools::xml::*;

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
    // The parser's line and column are in characters; the problem's offset is in bytes.
    let at = |text: &str| check(text).unwrap_err().offset;
    assert_eq!(
        at("<мир>\n<ab"),
        "<мир>\n<ab".len(),
        "the unclosed tag, at the end"
    );
    assert_eq!(at("<a>мир</b>"), "<a>мир</".len(), "the wrong closing tag");
    assert_eq!(at(""), 0, "nothing at all");
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
