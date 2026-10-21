use birchpad_tools::xml_schema::*;

/// The messages of the problems, each with the text at its place.
fn problems(found: Vec<Problem>, text: &str) -> Vec<(String, String)> {
    found
        .into_iter()
        .map(|problem| {
            let at: String = text[problem.offset..].chars().take(12).collect();
            (problem.message, at)
        })
        .collect()
}

const DTD: &str = r#"<?xml version="1.0"?>
<!DOCTYPE list [
  <!-- the list -->
  <!ELEMENT list (item+, note?)>
  <!ELEMENT item (#PCDATA | b)*>
  <!ELEMENT b (#PCDATA)>
  <!ELEMENT note EMPTY>
  <!ATTLIST item id ID #REQUIRED
                 kind (a | b) "a"
                 size NMTOKEN #IMPLIED>
  <!ATTLIST note ref IDREFS #IMPLIED version CDATA #FIXED "1">
]>
"#;

fn with_dtd(body: &str) -> String {
    format!("{DTD}{body}")
}

#[test]
fn reads_the_doctype() {
    let declared = doctype(DTD).unwrap();
    assert_eq!(declared.root, "list");
    assert_eq!(declared.system, None);
    assert!(declared.internal.unwrap().contains("<!ELEMENT note EMPTY>"));
    let system = doctype_of("<!DOCTYPE a SYSTEM \"a.dtd\"><a/>");
    assert_eq!(
        (system.root.as_str(), system.system.as_deref()),
        ("a", Some("a.dtd"))
    );
    let public = doctype_of("<!DOCTYPE a PUBLIC '-//X//EN' 'b.dtd' [<!ELEMENT a EMPTY>]><a/>");
    assert_eq!(public.system.as_deref(), Some("b.dtd"));
    assert_eq!(public.internal.as_deref(), Some("<!ELEMENT a EMPTY>"));
    assert_eq!(doctype("<a/>"), None);
    assert_eq!(doctype("<!DOCTYPE"), None);
}

fn doctype_of(text: &str) -> Doctype {
    doctype(text).unwrap()
}

#[test]
fn a_document_as_its_dtd_declares_is_valid() {
    let text = with_dtd(
        r#"<list><item id="i1" size="L">text <b>bold</b></item><item id="i2" kind="b"/><note ref="i1 i2" version="1"/></list>"#,
    );
    assert_eq!(validate_dtd(&text, None).unwrap(), []);
}

#[test]
fn finds_what_breaks_the_dtd() {
    let text = with_dtd(
        r#"<list><item id="i1" kind="c">x</item><item id="i1"><note/></item><note ref="nowhere" version="2" extra="1">no</note><stray/></list>"#,
    );
    let found = problems(validate_dtd(&text, None).unwrap(), &text);
    let messages: Vec<&str> = found.iter().map(|(message, _)| message.as_str()).collect();
    for expected in [
        "kind c is not one of a | b",
        "the ID i1 is given twice",
        "<note> is not allowed in <item>",
        "no element has the ID nowhere",
        "version must be 1",
        "the attribute extra is not declared for <note>",
        "<note> must be empty",
        "<stray> is not declared",
        "<stray> is not allowed in <list>",
    ] {
        assert!(messages.contains(&expected), "{expected}: {messages:#?}");
    }
    // In the document's order.
    let mut offsets: Vec<usize> = validate_dtd(&text, None)
        .unwrap()
        .iter()
        .map(|problem| problem.offset)
        .collect();
    let sorted = {
        let mut sorted = offsets.clone();
        sorted.sort_unstable();
        sorted
    };
    assert_eq!(offsets, sorted);
    offsets.dedup();
}

#[test]
fn checks_required_attributes_and_content_models() {
    let text = with_dtd("<list><item/></list>");
    let found = problems(validate_dtd(&text, None).unwrap(), &text);
    assert_eq!(found[0].0, "<item> needs the attribute id");
    // The order a sequence asks for.
    let text = with_dtd(r#"<list><note/><item id="a"/></list>"#);
    let found = problems(validate_dtd(&text, None).unwrap(), &text);
    assert_eq!(
        found[0].0,
        "the elements in <list> are not as declared: (item+, note?)"
    );
    // One item at least.
    let text = with_dtd("<list/>");
    assert_eq!(validate_dtd(&text, None).unwrap().len(), 1);
    // Text where only elements go.
    let text = with_dtd(r#"<list>words<item id="a"/></list>"#);
    let found = problems(validate_dtd(&text, None).unwrap(), &text);
    assert_eq!(
        found[0],
        (
            "<list> may not hold text".to_owned(),
            "words<item i".to_owned()
        )
    );
    // A root other than the DOCTYPE's.
    let text = format!("{DTD}<item id=\"a\"/>");
    let found = problems(validate_dtd(&text, None).unwrap(), &text);
    assert!(found[0].0.contains("DOCTYPE declares <list>"), "{found:?}");
}

#[test]
fn content_models_count_occurrences() {
    let model = |declaration: &str, body: &str| {
        let text = format!(
            "<!DOCTYPE r [<!ELEMENT r {declaration}><!ELEMENT a EMPTY><!ELEMENT b EMPTY>]>{body}"
        );
        validate_dtd(&text, None).unwrap().is_empty()
    };
    assert!(model("(a, b)", "<r><a/><b/></r>"));
    assert!(!model("(a, b)", "<r><b/><a/></r>"));
    assert!(!model("(a, b)", "<r><a/></r>"));
    assert!(model("(a | b)*", "<r/>"));
    assert!(model("(a | b)*", "<r><b/><a/><b/></r>"));
    assert!(model("(a?, b+)", "<r><b/><b/></r>"));
    assert!(!model("(a?, b+)", "<r><a/><a/><b/></r>"));
    assert!(model("((a, b)+)", "<r><a/><b/><a/><b/></r>"));
    assert!(!model("((a, b)+)", "<r><a/><b/><a/></r>"));
    assert!(model("ANY", "<r><a/>text<b/></r>"));
    assert!(model("(#PCDATA)", "<r>only text</r>"));
    assert!(!model("(#PCDATA)", "<r><a/></r>"));
    assert!(model("(a*)", &format!("<r>{}</r>", "<a/>".repeat(2000))));
}

#[test]
fn reads_an_external_dtd() {
    let text = "<!DOCTYPE r SYSTEM \"r.dtd\"><r><a/></r>";
    let external =
        "<!ELEMENT r (a)>\n<!ELEMENT a EMPTY>\n<!ENTITY % skipped \"x\">\n<!ELEMENT c (%skipped;)>";
    assert_eq!(validate_dtd(text, Some(external)).unwrap(), []);
    let wrong = "<!DOCTYPE r SYSTEM \"r.dtd\"><r><b/></r>";
    assert_eq!(validate_dtd(wrong, Some(external)).unwrap().len(), 2);
    // Without declarations nothing is undeclared.
    assert_eq!(
        validate_dtd("<!DOCTYPE r><r x=\"1\"><a/></r>", None).unwrap(),
        []
    );
}

#[test]
fn refuses_what_it_cannot_validate() {
    assert!(
        validate_dtd("<a/>", None)
            .unwrap_err()
            .to_string()
            .contains("DOCTYPE")
    );
    assert!(
        validate_dtd("<!DOCTYPE a><a>", None)
            .unwrap_err()
            .to_string()
            .contains("not XML")
    );
    assert!(
        validate_xsd("<a/>", "<notschema/>")
            .unwrap_err()
            .to_string()
            .contains("not an XML Schema")
    );
    assert!(
        validate_xsd("<a/>", "<xs:schema")
            .unwrap_err()
            .to_string()
            .contains("schema is not XML")
    );
    assert!(
        validate_xsd("<a", XSD)
            .unwrap_err()
            .to_string()
            .contains("not XML")
    );
}

const XSD: &str = r#"<?xml version="1.0"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="order" type="Order"/>
  <xs:complexType name="Order">
    <xs:sequence>
      <xs:element name="id" type="xs:positiveInteger"/>
      <xs:element name="when" type="xs:date"/>
      <xs:element ref="line" maxOccurs="unbounded"/>
      <xs:choice minOccurs="0">
        <xs:element name="paid" type="xs:boolean"/>
        <xs:element name="due" type="xs:dateTime"/>
      </xs:choice>
      <xs:element name="note" minOccurs="0" type="Note"/>
    </xs:sequence>
    <xs:attribute name="status" type="Status" use="required"/>
    <xs:attribute name="version" type="xs:string" fixed="2"/>
    <xs:attributeGroup ref="common"/>
  </xs:complexType>
  <xs:attributeGroup name="common">
    <xs:attribute name="lang" type="xs:language"/>
  </xs:attributeGroup>
  <xs:element name="line">
    <xs:complexType>
      <xs:simpleContent>
        <xs:extension base="Sku">
          <xs:attribute name="qty" use="required">
            <xs:simpleType>
              <xs:restriction base="xs:int">
                <xs:minInclusive value="1"/>
                <xs:maxExclusive value="100"/>
              </xs:restriction>
            </xs:simpleType>
          </xs:attribute>
        </xs:extension>
      </xs:simpleContent>
    </xs:complexType>
  </xs:element>
  <xs:simpleType name="Sku">
    <xs:restriction base="xs:string">
      <xs:pattern value="[A-Z]{2}-\d{3}"/>
    </xs:restriction>
  </xs:simpleType>
  <xs:simpleType name="Status">
    <xs:restriction base="xs:token">
      <xs:enumeration value="open"/>
      <xs:enumeration value="closed"/>
    </xs:restriction>
  </xs:simpleType>
  <xs:complexType name="Note" mixed="true">
    <xs:sequence>
      <xs:element name="b" type="xs:string" minOccurs="0" maxOccurs="unbounded"/>
    </xs:sequence>
  </xs:complexType>
</xs:schema>"#;

#[test]
fn a_document_as_its_schema_declares_is_valid() {
    let text = r#"<order status="open" version="2" lang="en" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:noNamespaceSchemaLocation="order.xsd">
  <id>12</id>
  <when>2026-10-21</when>
  <line qty="2">AB-123</line>
  <line qty="99">CD-456</line>
  <due>2026-11-01T12:00:00Z</due>
  <note>Leave it <b>at the door</b>, please.</note>
</order>"#;
    assert_eq!(validate_xsd(text, XSD).unwrap(), []);
    assert_eq!(schema_location(text).as_deref(), Some("order.xsd"));
}

#[test]
fn finds_what_breaks_the_schema() {
    let text = r#"<order status="pending" version="3" color="red">
  <id>-4</id>
  <when>2026-13-01</when>
  <line qty="100">ab-123</line>
  <line>XY-999</line>
  <paid>maybe</paid>
  <extra/>
</order>"#;
    let found = problems(validate_xsd(text, XSD).unwrap(), text);
    let messages: Vec<&str> = found.iter().map(|(message, _)| message.as_str()).collect();
    for expected in [
        "@status:  pending is not one of open | closed",
        "version must be 2",
        "the attribute color is not declared for <order>",
        "<id>: -4 is not a positiveInteger",
        "<when>: 2026-13-01 is not a date",
        "@qty: 100 is out of the range the schema allows",
        "<line>: ab-123 does not match the pattern [A-Z]{2}-\\d{3}",
        "<line> needs the attribute qty",
        "<paid>: maybe is not a boolean",
        "<extra> is not allowed in <order>",
    ] {
        assert!(messages.contains(&expected), "{expected}: {messages:#?}");
    }
}

#[test]
fn schema_content_models_and_roots() {
    // A required element missing, and the order.
    let missing = "<order status=\"open\"><id>1</id><line qty=\"1\">AB-123</line></order>";
    let found = problems(validate_xsd(missing, XSD).unwrap(), missing);
    assert!(
        found[0]
            .0
            .starts_with("the elements in <order> are not as declared"),
        "{found:?}"
    );
    // A root the schema does not declare.
    let found = validate_xsd("<line qty=\"1\">AB-123</line><!-- -->", XSD).unwrap();
    assert!(found.is_empty(), "a global element is a root: {found:?}");
    let found = validate_xsd("<id>1</id>", XSD).unwrap();
    assert_eq!(found[0].message, "the schema declares no element <id>");
    // Text in an element of elements only.
    let text = "<order status=\"open\">loose<id>1</id><when>2026-01-01</when><line qty=\"1\">AB-123</line></order>";
    let found = problems(validate_xsd(text, XSD).unwrap(), text);
    assert_eq!(found[0].0, "<order> may not hold text");
}

#[test]
fn all_groups_lists_and_built_in_types() {
    let schema = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="r">
    <xs:complexType>
      <xs:all>
        <xs:element name="a" type="xs:byte"/>
        <xs:element name="b" type="Numbers" minOccurs="0"/>
        <xs:element name="c" type="xs:time"/>
      </xs:all>
      <xs:anyAttribute/>
    </xs:complexType>
  </xs:element>
  <xs:simpleType name="Numbers">
    <xs:list itemType="xs:decimal"/>
  </xs:simpleType>
</xs:schema>"#;
    let valid = |body: &str| validate_xsd(&format!("<r any=\"1\">{body}</r>"), schema).unwrap();
    assert_eq!(valid("<c>23:59:59</c><a>-128</a>"), []);
    assert_eq!(valid("<b>1 2.5 -3</b><c>00:00:00</c><a>127</a>"), []);
    assert_eq!(valid("<a>128</a><c>12:00:00</c>").len(), 1);
    assert_eq!(valid("<a>1</a><c>12:00:00</c><a>2</a>").len(), 1);
    assert_eq!(valid("<a>1</a>").len(), 1);
    assert_eq!(valid("<a>1</a><b>1 x</b><c>25:00:00</c>").len(), 2);
}

#[test]
fn built_in_types_at_their_limits() {
    let check = |kind: &str, value: &str| {
        let schema = format!(
            r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:element name="v" type="xs:{kind}"/></xs:schema>"#
        );
        validate_xsd(&format!("<v>{value}</v>"), &schema)
            .unwrap()
            .is_empty()
    };
    for (kind, good, bad) in [
        ("int", "2147483647", "2147483648"),
        ("int", "-2147483648", "-2147483649"),
        ("unsignedByte", "255", "256"),
        ("unsignedByte", "0", "-1"),
        ("nonNegativeInteger", "0", "-1"),
        ("negativeInteger", "-1", "0"),
        ("decimal", "-.5", "1e3"),
        ("double", "INF", "infinity"),
        ("float", "1e3", "abc"),
        ("boolean", "1", "yes"),
        ("date", "2024-02-29Z", "2024-2-29"),
        (
            "dateTime",
            "2026-10-21T08:30:00+03:00",
            "2026-10-21 08:30:00",
        ),
        ("time", "24:00:00", "12:60:00"),
        ("NCName", "a-b", "a:b"),
        ("hexBinary", "0aFF", "0aF"),
        ("string", "  anything &lt;at&gt; all  ", "\u{0}"),
    ] {
        assert!(check(kind, good), "{kind} {good}");
        if kind != "string" {
            assert!(!check(kind, bad), "{kind} {bad}");
        }
    }
}

#[test]
fn recursive_types_and_deep_documents_end() {
    let schema = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="node" type="Node"/>
  <xs:complexType name="Node">
    <xs:sequence><xs:element name="node" type="Node" minOccurs="0" maxOccurs="unbounded"/></xs:sequence>
  </xs:complexType>
</xs:schema>"#;
    let deep = format!("{}{}", "<node>".repeat(200), "</node>".repeat(200));
    assert_eq!(validate_xsd(&deep, schema).unwrap(), []);
    let wrong = format!("{}<leaf/>{}", "<node>".repeat(50), "</node>".repeat(50));
    assert_eq!(validate_xsd(&wrong, schema).unwrap().len(), 1);
}

#[test]
fn problems_stop_at_the_limit() {
    let text = format!(
        "<!DOCTYPE r [<!ELEMENT r (a)*><!ELEMENT a EMPTY>]><r>{}</r>",
        "<b/>".repeat(MAX_PROBLEMS * 2)
    );
    assert_eq!(validate_dtd(&text, None).unwrap().len(), MAX_PROBLEMS);
}

#[test]
fn the_schema_location_and_the_tag_of_a_problem() {
    let pairs = r#"<r xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:schemaLocation="urn:a a.xsd urn:b b.xsd"/>"#;
    assert_eq!(schema_location(pairs).as_deref(), Some("a.xsd"));
    assert_eq!(schema_location("<r/>"), None);
    assert_eq!(schema_location("<r"), None);
    let text = "<a><b x=\"1\">t</b></a>";
    assert_eq!(&text[tag_at(text, 3)], "<b x=\"1\">");
    assert_eq!(tag_at(text, text.len()), text.len()..text.len());
    assert_eq!(&text[tag_at("<a", 0)], "<a");
}
