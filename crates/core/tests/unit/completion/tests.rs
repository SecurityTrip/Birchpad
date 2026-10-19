use super::*;

#[test]
fn words_that_start_with_the_prefix() {
    let text = Rope::from_str("print printf Printer prin pr_x 2print прибор при");
    let all = word_candidates(&text, "pri", 21..25, 10);
    assert_eq!(all, ["print", "Printer", "printf"]);
    assert_eq!(word_candidates(&text, "при", 0..0, 10), ["прибор"]);
    // The limit, an empty prefix, and nothing found.
    assert_eq!(word_candidates(&text, "pri", 21..25, 1), ["print"]);
    assert!(word_candidates(&text, "", 0..0, 10).is_empty());
    assert!(word_candidates(&text, "zz", 0..0, 10).is_empty());
    assert!(word_candidates(&text, "pri", 0..0, 0).is_empty());
}

#[test]
fn the_word_typed_is_left_out_unless_it_is_elsewhere() {
    let text = Rope::from_str("prints prin");
    assert_eq!(word_candidates(&text, "prin", 7..11, 10), ["prints"]);
    let text = Rope::from_str("prints");
    assert!(word_candidates(&text, "p", 0..6, 10).is_empty());
    let text = Rope::from_str("prints prints");
    assert_eq!(word_candidates(&text, "p", 0..6, 10), ["prints"]);
}

#[test]
fn case_and_numbers_as_the_options_say() {
    let text = Rope::from_str("Print print 2print 20 200");
    let exact = WordOptions {
        ignore_case: false,
        ignore_numbers: true,
    };
    assert_eq!(
        word_candidates_with(&text, "pr", 0..0, 10, exact),
        ["print"]
    );
    assert_eq!(
        word_candidates_with(&text, "Pr", 0..0, 10, exact),
        ["Print"],
        "case matters"
    );
    let numbers = WordOptions {
        ignore_case: true,
        ignore_numbers: false,
    };
    assert_eq!(
        word_candidates_with(&text, "2", 0..0, 10, numbers),
        ["20", "200", "2print"]
    );
    assert!(
        word_candidates(&text, "2", 0..0, 10).is_empty(),
        "numbers ignored by default"
    );
    assert!(word_candidates_with(&text, "", 0..0, 10, numbers).is_empty());
}

#[test]
fn opening_tags_are_closed() {
    assert_eq!(closing_tag("<div>", true).as_deref(), Some("</div>"));
    assert_eq!(
        closing_tag("text <a href=\"x\" class='y'>", true).as_deref(),
        Some("</a>")
    );
    assert_eq!(
        closing_tag("<ns:item>", false).as_deref(),
        Some("</ns:item>")
    );
    assert_eq!(
        closing_tag("<x-y_z.1>", false).as_deref(),
        Some("</x-y_z.1>")
    );
    assert_eq!(
        closing_tag("<Br>", false).as_deref(),
        Some("</Br>"),
        "XML has no void elements"
    );
}

#[test]
fn other_tags_are_not_closed() {
    for typed in [
        "</div>",
        "<br/>",
        "<br />",
        "<!-- c -->",
        "<!DOCTYPE html>",
        "<?xml version=\"1.0\"?>",
        "a > b",
        ">",
        "<>",
        "< div>",
        "<1a>",
        "<a title=\"x>",
        "<a title='x>",
        "<div><>",
    ] {
        assert_eq!(closing_tag(typed, true), None, "{typed}");
    }
    assert_eq!(closing_tag("<br>", true), None, "a void element");
    assert_eq!(closing_tag("<IMG src=x>", true), None);
    assert_eq!(closing_tag("<div", true), None, "no > typed");
    assert_eq!(closing_tag("", true), None);
}

#[test]
fn word_starts() {
    let text = Rope::from_str("a.foo_bar1 д");
    assert_eq!(word_start(&text, 10), 2);
    assert_eq!(word_start(&text, 2), 2);
    assert_eq!(word_start(&text, 0), 0);
    assert_eq!(word_start(&text, text.len()), 11);
    assert_eq!(word_start(&text, 999), 11);
}
