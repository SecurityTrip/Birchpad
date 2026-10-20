use birchpad_core::search::*;
use ropey::Rope;

proptest::proptest! {
    #[test]
    fn replace_all_does_what_a_plain_replace_does(
        source in "[ab \n]{0,200}",
        needle in "[ab]{1,3}",
        replacement in "[xy]{0,3}",
        regex: bool,
    ) {
        let text = Rope::from_str(&source);
        // As a regular expression, the needle has nothing special in it.
        let query = Query {
            pattern: needle.clone(),
            match_case: true,
            mode: if regex { SearchMode::Regex } else { SearchMode::Normal },
            ..Query::default()
        };
        let searcher = Searcher::new(&query).unwrap();
        let mut replaced = String::new();
        let mut at = 0;
        for (range, with) in searcher.replacements(&text, 0..text.len(), &replacement) {
            proptest::prop_assert!(range.start >= at);
            replaced.push_str(&source[at..range.start]);
            replaced.push_str(&with);
            at = range.end;
        }
        replaced.push_str(&source[at..]);
        proptest::prop_assert_eq!(replaced, source.replace(&needle, &replacement));
    }
}
