use birchpad_core::fuzzy::*;

fn positions(query: &str, text: &str) -> Vec<usize> {
    find(query, text).expect("matches").positions
}

fn score(query: &str, text: &str) -> i32 {
    find(query, text).expect("matches").score
}

fn ranked<'a>(query: &str, texts: &[&'a str]) -> Vec<&'a str> {
    rank(query, texts.iter().copied())
        .into_iter()
        .map(|(index, _)| texts[index])
        .collect()
}

#[test]
fn characters_in_order_match_without_regard_to_case() {
    assert_eq!(positions("sa", "Save As..."), [0, 1]);
    assert_eq!(positions("SAVE", "save all"), [0, 1, 2, 3]);
    assert!(find("sva", "Save").is_none());
    assert!(find("vs", "Save").is_none());
}

#[test]
fn word_starts_win_over_characters_inside_words() {
    assert_eq!(positions("ca", "Close All"), [0, 6]);
    assert!(score("ca", "Close All") > score("ca", "Recap"));
    assert_eq!(positions("ws", "WordStar"), [0, 4]);
}

#[test]
fn adjacent_characters_win_over_scattered_ones() {
    assert!(score("save", "Save") > score("save", "Select A Visual Element"));
    assert_eq!(positions("ave", "axvxe ave"), [6, 7, 8]);
}

#[test]
fn every_term_matches_on_its_own() {
    let path = "crates/app/src/workspace.rs";
    let found = positions("work app", path);
    assert!(found.contains(&7) && found.contains(&15), "{found:?}");
    assert!(find("work zzz", path).is_none());
    // Terms may use the same characters.
    assert_eq!(positions("a a", "a"), [0]);
}

#[test]
fn an_empty_query_matches_everything_with_nothing_marked() {
    for query in ["", "   ", "\t"] {
        let found = find(query, "Save").unwrap();
        assert_eq!((found.score, found.positions), (0, vec![]));
    }
    assert!(find("", "").is_some());
    assert!(find("a", "").is_none());
}

#[test]
fn positions_are_byte_offsets_of_wide_characters() {
    assert_eq!(positions("жь", "Ёжик, ёжь"), [12, 14]);
    assert_eq!(positions("ё", "Ёлка"), [0]);
    assert_eq!(positions("界", "世界.txt"), [3]);
}

#[test]
fn a_query_longer_than_the_text_does_not_match() {
    assert!(find("saves", "save").is_none());
    assert_eq!(positions("save", "save"), [0, 1, 2, 3]);
}

#[test]
fn very_long_texts_still_match() {
    let long = format!("{}needle", "x".repeat(100_000));
    let found = find("needle", &long).unwrap();
    assert_eq!(found.positions.first(), Some(&100_000));
    assert_eq!(found.positions.len(), 6);
}

#[test]
fn rank_puts_the_best_first_then_shorter_then_given_order() {
    let texts = ["Select All", "Save As...", "Save", "Save All"];
    assert_eq!(ranked("save", &texts), ["Save", "Save All", "Save As..."]);
    assert_eq!(ranked("", &texts), texts);
    assert_eq!(ranked("  ", &texts), texts);
    assert!(ranked("zzz", &texts).is_empty());
    assert!(rank("save", std::iter::empty()).is_empty());
    // Equal scores and lengths keep the order given.
    assert_eq!(ranked("a", &["ab", "ac"]), ["ab", "ac"]);
}

#[test]
fn rank_recent_puts_the_recent_matches_first_in_their_order() {
    let texts = ["Save All", "Select All", "Save", "Save As..."];
    let order = |query: &str, recent: usize| -> Vec<&str> {
        rank_recent(query, texts.iter().copied(), recent)
            .into_iter()
            .map(|(index, _)| texts[index])
            .collect()
    };
    // The first two were used last; "Save" ranks better but was not used.
    assert_eq!(order("save", 2), ["Save All", "Save", "Save As..."]);
    assert_eq!(
        order("", 2),
        ["Save All", "Select All", "Save", "Save As..."]
    );
    // No recent ones, or all of them.
    assert_eq!(order("save", 0), ["Save", "Save All", "Save As..."]);
    assert_eq!(order("save", 4), ["Save All", "Save", "Save As..."]);
    // More recent than texts.
    assert_eq!(order("select", 10), ["Select All"]);
    assert!(order("zzz", 2).is_empty());
}
