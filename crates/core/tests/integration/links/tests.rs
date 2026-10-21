use birchpad_core::links::*;
use ropey::Rope;

fn found<'a>(line: &'a str, extra: &[String]) -> Vec<&'a str> {
    links_in(line, extra)
        .into_iter()
        .map(|range| &line[range])
        .collect()
}

#[test]
fn addresses_in_a_line() {
    assert_eq!(
        found("see https://example.com/a?b=1#c, or HTTP://X.ORG.", &[]),
        ["https://example.com/a?b=1#c", "HTTP://X.ORG"]
    );
    assert_eq!(
        found("(https://en.wikipedia.org/wiki/Rust_(language)) end", &[]),
        ["https://en.wikipedia.org/wiki/Rust_(language)"]
    );
    assert_eq!(
        found("<a href=\"http://a.b/c\">mailto:me@x.y</a>", &[]),
        ["http://a.b/c", "mailto:me@x.y"]
    );
    assert_eq!(
        found("file:///C:/notes/a.txt", &[]),
        ["file:///C:/notes/a.txt"]
    );
    assert_eq!(found("http://é.рф/путь", &[]), ["http://é.рф/путь"]);
}

#[test]
fn what_is_not_a_link() {
    assert!(found("", &[]).is_empty());
    assert!(found("http:// nothing after", &[]).is_empty());
    assert!(found("xhttp://a.b", &[]).is_empty());
    assert!(found("svn://repo", &[]).is_empty());
    assert!(found("https://...", &[]).is_empty());
}

#[test]
fn schemes_of_ones_own() {
    let extra = vec!["svn://".to_owned(), "".to_owned(), "Git://".to_owned()];
    assert_eq!(
        found("svn://repo/trunk git://host/x", &extra),
        ["svn://repo/trunk", "git://host/x"]
    );
}

#[test]
fn the_link_at_a_position() {
    let text = Rope::from("one\nsee https://a.b/c.\nend");
    assert_eq!(link_at(&text, 8, &[]), Some(8..21));
    assert_eq!(link_at(&text, 21, &[]), Some(8..21), "at its end");
    assert_eq!(link_at(&text, 5, &[]), None);
    assert_eq!(link_at(&text, 999, &[]), None);
}

#[test]
fn between_delimiters_on_a_line_or_across() {
    let text = Rope::from("say \"hello world\" now\n\"two\nlines\"");
    assert_eq!(between_delimiters(&text, 8, '"', '"', false), Some(5..16));
    assert_eq!(between_delimiters(&text, 5, '"', '"', false), Some(5..16));
    // No closing one on the line.
    assert_eq!(between_delimiters(&text, 24, '"', '"', false), None);
    assert_eq!(between_delimiters(&text, 24, '"', '"', true), Some(23..32));
    assert_eq!(between_delimiters(&text, 1, '"', '"', false), None);
    let brackets = Rope::from("f(a, (b))");
    assert_eq!(
        between_delimiters(&brackets, 7, '(', ')', false),
        Some(6..7)
    );
    assert_eq!(between_delimiters(&Rope::new(), 0, '(', ')', true), None);
}
