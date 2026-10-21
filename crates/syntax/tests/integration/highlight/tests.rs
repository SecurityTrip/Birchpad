use birchpad_syntax::*;

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
