use super::*;

fn all() -> Pairs {
    Pairs(vec![
        ('(', ')'),
        ('[', ']'),
        ('{', '}'),
        ('"', '"'),
        ('\'', '\''),
    ])
}

/// `text` with `|` for carets, typed `ch`: the result with its carets.
fn typed(text: &str, ch: char) -> Option<String> {
    let (rope, selection) = parse(text);
    let transaction = type_with_pairs(&rope, &selection, ch, &all())?;
    Some(show(&rope, &transaction))
}

fn backspace(text: &str) -> Option<String> {
    let (rope, selection) = parse(text);
    let transaction = delete_pairs(&rope, &selection, &all())?;
    Some(show(&rope, &transaction))
}

fn parse(text: &str) -> (Rope, Selection) {
    let mut carets = Vec::new();
    let mut plain = String::new();
    for ch in text.chars() {
        if ch == '|' {
            carets.push(Span::point(plain.len()));
        } else {
            plain.push(ch);
        }
    }
    (Rope::from_str(&plain), Selection::new(carets, 0))
}

fn show(rope: &Rope, transaction: &Transaction) -> String {
    let mut result = rope.clone();
    transaction.changes().apply(&mut result);
    let mut text = result.to_string();
    let mut heads: Vec<usize> = transaction
        .selection()
        .unwrap()
        .iter()
        .map(|range| range.head)
        .collect();
    heads.sort_unstable();
    for head in heads.into_iter().rev() {
        text.insert(head, '|');
    }
    text
}

#[test]
fn opening_brackets_get_their_partner() {
    assert_eq!(typed("f|", '(').as_deref(), Some("f(|)"));
    assert_eq!(typed("|", '[').as_deref(), Some("[|]"));
    assert_eq!(typed("a | b", '{').as_deref(), Some("a {|} b"));
    assert_eq!(typed("(|)", '(').as_deref(), Some("((|))"));
    // Before a word, the bracket is typed alone.
    assert_eq!(typed("|word", '('), None);
}

#[test]
fn closing_characters_step_over() {
    assert_eq!(typed("f(|)", ')').as_deref(), Some("f()|"));
    assert_eq!(typed("\"a|\"", '"').as_deref(), Some("\"a\"|"));
    assert_eq!(typed("|x", ')'), None);
    assert_eq!(typed("|", ')'), None);
}

#[test]
fn quotes_pair_only_where_a_string_starts() {
    assert_eq!(typed("x = |", '"').as_deref(), Some("x = \"|\""));
    // An apostrophe after a word.
    assert_eq!(typed("don|", '\''), None);
    assert_eq!(typed("é|", '\''), None);
    assert_eq!(typed("x = |", '\'').as_deref(), Some("x = '|'"));
}

#[test]
fn every_caret_at_once() {
    assert_eq!(typed("a|\nb|\nc", '(').as_deref(), Some("a(|)\nb(|)\nc"));
    // Carets that pair and carets that type.
    assert_eq!(typed("|x\n|", '(').as_deref(), Some("(|x\n(|)"));
    // Wide characters and the end of the text.
    assert_eq!(typed("日|", '[').as_deref(), Some("日[|]"));
}

#[test]
fn a_selection_types_as_usual() {
    let rope = Rope::from_str("abc");
    let selection = Selection::new(vec![Span::new(0, 2)], 0);
    assert!(type_with_pairs(&rope, &selection, '(', &all()).is_none());
    assert!(delete_pairs(&rope, &selection, &all()).is_none());
    // No pairs set: nothing pairs.
    let rope = Rope::from_str("");
    assert!(type_with_pairs(&rope, &Selection::point(0), '(', &Pairs::default()).is_none());
}

#[test]
fn backspace_deletes_an_empty_pair() {
    assert_eq!(backspace("f(|)").as_deref(), Some("f|"));
    assert_eq!(backspace("\"|\"").as_deref(), Some("|"));
    assert_eq!(backspace("[|]\n{|}").as_deref(), Some("|\n|"));
    assert_eq!(backspace("(|x)"), None);
    assert_eq!(backspace("|"), None);
    assert_eq!(backspace("(|"), None);
    assert_eq!(backspace("(|]"), None);
    // One caret not in a pair: the usual Backspace for all.
    assert_eq!(backspace("(|)\nx|"), None);
}
