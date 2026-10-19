#![allow(
    clippy::single_range_in_vec_init,
    reason = "lists of one expected match"
)]

use super::*;

fn searcher(pattern: &str, match_case: bool, whole_word: bool) -> Searcher {
    Searcher::new(&Query {
        pattern: pattern.into(),
        match_case,
        whole_word,
        ..Query::default()
    })
    .unwrap()
}

fn mode(pattern: &str, mode: SearchMode) -> Searcher {
    Searcher::new(&Query {
        pattern: pattern.into(),
        match_case: true,
        mode,
        ..Query::default()
    })
    .unwrap()
}

fn regex(pattern: &str) -> Searcher {
    mode(pattern, SearchMode::Regex)
}

fn matched(searcher: &Searcher, text: &Rope) -> Vec<String> {
    searcher
        .find_all(text)
        .into_iter()
        .map(|range| text.slice(range).to_string())
        .collect()
}

#[test]
fn extended_mode_reads_notepad_plus_plus_escapes() {
    assert_eq!(
        unescape_extended(r"a\tb\nc\\d\x41Ж\o101\d065\b01000001\q\"),
        "a\tb\nc\\dAЖAAA\\q\\"
    );
    assert_eq!(
        unescape_extended(r"\x4"),
        r"\x4",
        "too few digits stay as written"
    );
    let text = Rope::from_str("one\r\ntwo\tthree");
    let s = mode(r"\r\ntwo\t", SearchMode::Extended);
    assert_eq!(s.find_all(&text), [3..9]);
    assert_eq!(s.replacement(&text, 3..9, r"\n-"), "\n-");
    assert_eq!(
        mode(r"\r\n", SearchMode::Normal).find_all(&text),
        Vec::<Range<usize>>::new(),
        "Normal mode is literal"
    );
}

#[test]
fn regular_expressions_have_back_references_and_look_around() {
    let text = Rope::from_str("the the cat sat on on the mat\nprice: $42, cost: $7");
    assert_eq!(
        matched(&regex(r"\b(\w+) \1\b"), &text),
        ["the the", "on on"]
    );
    assert_eq!(matched(&regex(r"(?<=\$)\d+"), &text), ["42", "7"]);
    assert_eq!(matched(&regex(r"\w+(?=:)"), &text), ["price", "cost"]);
    let insensitive = Searcher::new(&Query {
        pattern: "ПРИВЕТ|CAT".into(),
        mode: SearchMode::Regex,
        ..Query::default()
    })
    .unwrap();
    assert_eq!(
        matched(&insensitive, &Rope::from_str("привет, Cat")),
        ["привет", "Cat"]
    );
}

#[test]
fn regular_expressions_work_line_by_line_with_any_line_ending() {
    let text = Rope::from_str("a1\r\nb2\r\nc3");
    assert_eq!(matched(&regex(r"^\w"), &text), ["a", "b", "c"]);
    assert_eq!(matched(&regex(r"\d$"), &text), ["1", "2", "3"]);
    assert_eq!(matched(&regex(r"1.+b"), &text), Vec::<String>::new());
    let across = Searcher::new(&Query {
        pattern: r"1.+b".into(),
        match_case: true,
        mode: SearchMode::Regex,
        dot_matches_newline: true,
        ..Query::default()
    })
    .unwrap();
    assert_eq!(matched(&across, &text), ["1\r\nb"]);
}

#[test]
fn empty_matches_replace_like_notepad_plus_plus() {
    // Commenting out every line: `^` matches at each line start, the empty last line too.
    let text = Rope::from_str("x\ny\n");
    let replaced: Vec<(Range<usize>, String)> =
        regex("^").replacements(&text, 0..text.len(), "// ");
    assert_eq!(
        replaced,
        [
            (0..0, "// ".to_owned()),
            (2..2, "// ".to_owned()),
            (4..4, "// ".to_owned())
        ]
    );
    // No empty match right where a match ended.
    assert_eq!(regex("a*").find_all(&Rope::from_str("aab")), [0..2, 3..3]);
}

#[test]
fn replacements_expand_groups_and_case() {
    let group = |i: usize| ["me@host", "me", "host"].get(i).copied();
    let named = |name: &str| (name == "user").then_some("me");
    let expand = |template: &str| expand_replacement(template, group, named);
    assert_eq!(expand(r"$2 at ${1}"), "host at me");
    assert_eq!(expand(r"\2 \1 $& $0"), "host me me@host me@host");
    assert_eq!(expand(r"${user}!"), "me!");
    assert_eq!(expand(r"\U$1\E-$2 \u$2 \L\uHELLO"), "ME-host Host Hello");
    assert_eq!(
        expand(r"$$5 $9 \t\\"),
        "$5  \t\\",
        "a missing group is empty"
    );
    let text = Rope::from_str("me@host, you@there");
    let s = regex(r"(?<user>\w+)@(\w+)");
    assert_eq!(
        s.replacements(&text, 0..text.len(), r"${user} at \U$2"),
        [
            (0..7, "me at HOST".to_owned()),
            (9..18, "you at THERE".to_owned())
        ]
    );
    assert_eq!(s.replacement(&text, 9..18, "[$1]"), "[you]");
}

#[test]
fn regular_expressions_in_a_selection_see_the_text_around_it() {
    let text = Rope::from_str("a1 b2 c3");
    let s = regex(r"\w\d");
    assert_eq!(s.find_all_in(&text, 3..5), [3..5]);
    assert_eq!(
        s.find_all_in(&text, 3..4),
        Vec::<Range<usize>>::new(),
        "a match must end in the range"
    );
    // Look-behind sees what precedes the selection.
    assert_eq!(regex(r"(?<=1 )\w").find_all_in(&text, 3..8), [3..4]);
    assert_eq!(s.find(&text, 5, Direction::Backward, false), Some(3..5));
    assert_eq!(s.find(&text, 1, Direction::Backward, true), Some(6..8));
    assert!(regex(r"\d+").is_match(&Rope::from_str("ab123"), 2..5));
    assert!(!regex(r"\d+").is_match(&Rope::from_str("ab123"), 1..5));
    let mut scan = Scan::with_step(0..text.len(), 2);
    while !scan.run(&s, &text, || true) {}
    assert_eq!(scan.matches(), s.find_all(&text).as_slice());
}

#[test]
fn invalid_regular_expressions_are_errors() {
    let error = Searcher::new(&Query {
        pattern: "(unclosed".into(),
        mode: SearchMode::Regex,
        ..Query::default()
    })
    .unwrap_err();
    assert!(matches!(error, SearchError::InvalidRegex(_)), "{error}");
    assert_eq!(regex("a").failure(), None);
}

#[test]
fn finds_forward_and_backward_with_wrap() {
    let text = Rope::from_str("one two one three one");
    let s = searcher("one", true, false);
    assert_eq!(s.find(&text, 0, Direction::Forward, false), Some(0..3));
    assert_eq!(s.find(&text, 1, Direction::Forward, false), Some(8..11));
    assert_eq!(s.find(&text, 19, Direction::Forward, false), None);
    assert_eq!(s.find(&text, 19, Direction::Forward, true), Some(0..3));
    assert_eq!(s.find(&text, 18, Direction::Backward, false), Some(8..11));
    assert_eq!(s.find(&text, 2, Direction::Backward, false), None);
    assert_eq!(s.find(&text, 2, Direction::Backward, true), Some(18..21));
    assert_eq!(s.find_all(&text), [0..3, 8..11, 18..21]);
}

#[test]
fn case_and_whole_word() {
    let text = Rope::from_str("Word word swords WORD Привет привет");
    assert_eq!(
        searcher("word", true, false).find_all(&text),
        [5..9, 11..15]
    );
    assert_eq!(
        searcher("word", false, false).find_all(&text),
        [0..4, 5..9, 11..15, 17..21]
    );
    assert_eq!(
        searcher("word", false, true).find_all(&text),
        [0..4, 5..9, 17..21]
    );
    assert_eq!(searcher("ПРИВЕТ", false, true).find_all(&text).len(), 2);
}

#[test]
fn matches_across_chunk_boundaries() {
    // Ropey chunks are around a kilobyte; put needles at every offset around boundaries.
    let mut source = String::new();
    for i in 0..2000 {
        source.push_str(if i % 97 == 0 { "needle" } else { "hay." });
    }
    let text = Rope::from_str(&source);
    let expected: Vec<Range<usize>> = source
        .match_indices("needle")
        .map(|(at, m)| at..at + m.len())
        .collect();
    assert_eq!(searcher("needle", true, false).find_all(&text), expected);
    assert_eq!(searcher("NEEDLE", false, false).find_all(&text), expected);
    let last = expected.last().unwrap().clone();
    assert_eq!(
        searcher("needle", true, false).find(&text, text.len(), Direction::Backward, false),
        Some(last)
    );
}

#[test]
fn empty_pattern_is_an_error() {
    assert_eq!(
        Searcher::new(&Query::default()).unwrap_err(),
        SearchError::Empty
    );
    // In every mode and with match case: an Extended pattern of nothing is empty too.
    for mode in [SearchMode::Normal, SearchMode::Extended] {
        for match_case in [false, true] {
            let query = Query {
                mode,
                match_case,
                ..Query::default()
            };
            assert_eq!(Searcher::new(&query).unwrap_err(), SearchError::Empty);
        }
    }
}

/// A pattern fancy-regex can only run by backtracking (a back-reference), on a text where
/// that takes longer than the limit allows.
fn runaway() -> (Searcher, Rope) {
    let text = Rope::from_str(&format!("{}!", "a".repeat(64)));
    (regex(r"^(a|a)*\1b"), text)
}

#[test]
fn a_runaway_regular_expression_fails_instead_of_finding_nothing() {
    let (s, text) = runaway();
    assert_eq!(s.failure(), None, "nothing has run yet");
    assert_eq!(s.find_in(&text, 0..text.len()), None);
    assert!(matches!(s.failure(), Some(SearchError::RegexFailed(_))));
    assert_eq!(s.find_last_in(&text, 0..text.len()), None);
    assert!(s.failure().is_some());
    assert!(s.replacements(&text, 0..text.len(), "x").is_empty());
    assert!(!s.is_match(&text, 0..text.len()));
    // A search that ends normally clears the failure.
    let fine = regex("a");
    assert!(fine.find_in(&text, 0..text.len()).is_some());
    assert_eq!(fine.failure(), None);
}

#[test]
fn searching_backward_finds_a_match_far_before_the_end() {
    // The backward search looks in growing windows from the end: the only match is at
    // the very start, many windows away; another is cut by the end of the range.
    let source = format!("needle{}needle", "-".repeat(100_000));
    let text = Rope::from_str(&source);
    let s = searcher("needle", true, false);
    assert_eq!(s.find_last_in(&text, 0..text.len() - 1), Some(0..6));
    assert_eq!(s.find_last_in(&text, 1..text.len() - 1), None);
    assert_eq!(s.find_last_in(&text, 0..0), None, "an empty range");
    assert_eq!(
        s.find_last_in(&text, 0..text.len()),
        Some(text.len() - 6..text.len())
    );
}

#[test]
fn extended_escapes_at_their_limits() {
    // A backslash at the very end, codes with too few or wrong digits, and the largest
    // and smallest codes.
    assert_eq!(unescape_extended("end\\"), "end\\");
    assert_eq!(unescape_extended(r"\u41"), r"\u41");
    assert_eq!(unescape_extended(r"\xZZ"), r"\xZZ");
    assert_eq!(unescape_extended(r"\o9"), r"\o9");
    assert_eq!(unescape_extended(r"\d25"), r"\d25");
    assert_eq!(unescape_extended(r"\xff\x00"), "\u{ff}\0");
    assert_eq!(unescape_extended(r"\uffff"), "\u{ffff}");
    assert_eq!(unescape_extended(r"\o377\d255"), "\u{ff}\u{ff}");
    // A surrogate is no character: it stays as written.
    assert_eq!(unescape_extended(r"\ud800"), r"\ud800");
    assert_eq!(unescape_extended(""), "");
}

#[test]
fn replacement_templates_at_their_limits() {
    let group = |i: usize| ["whole", "one"].get(i).copied();
    let named = |_: &str| None;
    let expand = |template: &str| expand_replacement(template, group, named);
    // A `$` or a backslash at the very end is kept; `$` before a non-group is literal.
    assert_eq!(expand("cost $"), "cost $");
    assert_eq!(expand("path\\"), "path\\");
    assert_eq!(expand("$x"), "$x");
    // An unknown name, an unclosed name, group 0 and the empty template.
    assert_eq!(expand("${nope}"), "");
    assert_eq!(expand("${one"), "${one");
    assert_eq!(expand(r"\0"), "whole");
    assert_eq!(expand(""), "");
    // Line breaks are not case-converted, and \E ends a conversion.
    assert_eq!(expand(r"\Ua\nb\Ec"), "A\nBc");
    assert_eq!(expand(r"\l\U$1"), "oNE");
}

/// Where `needle` occurs in `haystack`, without overlaps, by the plainest means.
fn naive(haystack: &str, needle: &str, match_case: bool) -> Vec<Range<usize>> {
    let (haystack, needle) = if match_case {
        (haystack.to_owned(), needle.to_owned())
    } else {
        (haystack.to_lowercase(), needle.to_lowercase())
    };
    haystack
        .match_indices(&needle)
        .map(|(at, found)| at..at + found.len())
        .collect()
}

proptest::proptest! {
    #[test]
    fn normal_search_finds_what_a_plain_search_finds(
        pieces in proptest::collection::vec("[abAB \n]{0,40}", 1..60),
        needle in "[abAB]{1,4}",
        match_case: bool,
    ) {
        // Built from pieces so that chunk boundaries fall all over the text.
        let source = pieces.concat();
        let mut text = Rope::new();
        for piece in &pieces {
            let end = text.len();
            text.insert(end, piece);
        }
        let found = searcher(&needle, match_case, false).find_all(&text);
        proptest::prop_assert_eq!(found, naive(&source, &needle, match_case));
    }

    #[test]
    fn a_backward_search_finds_the_last_forward_match(
        source in "[ab \n]{0,300}",
        needle in "[ab]{1,3}",
    ) {
        let text = Rope::from_str(&source);
        let s = searcher(&needle, true, false);
        let all = s.find_all(&text);
        let last = s.find_last_in(&text, 0..text.len());
        // The last match backward may overlap the last forward one ("aa" in "aaa").
        match (all.last(), last) {
            (None, None) => {}
            (Some(forward), Some(backward)) => {
                proptest::prop_assert!(backward.start >= forward.start);
                proptest::prop_assert_eq!(&source[backward.clone()], needle.as_str());
            }
            (forward, backward) => proptest::prop_assert!(
                false,
                "forward {:?}, backward {:?}",
                forward,
                backward
            ),
        }
    }
}

#[test]
fn is_match_checks_the_exact_range() {
    let text = Rope::from_str("abc abc");
    let s = searcher("abc", true, false);
    assert!(s.is_match(&text, 4..7));
    assert!(!s.is_match(&text, 3..7));
}

#[test]
fn a_scan_in_small_steps_finds_what_one_search_finds() {
    let text = Rope::from_str(&"one two one, ONE.  one".repeat(50));
    let s = searcher("one", false, true);
    let whole = s.find_all_in(&text, 3..text.len() - 2);
    assert_eq!(whole.first(), Some(&(8..11)));
    for step in [1, 2, 5, 64] {
        let mut scan = Scan::with_step(3..text.len() - 2, step);
        let mut steps = 0;
        while !scan.run(&s, &text, || true) {
            steps += 1;
        }
        assert!(steps > 0 || step == 64);
        assert_eq!(scan.matches(), whole.as_slice(), "step {step}");
    }
}

#[test]
fn smart_highlighting_wants_a_word_or_one_line() {
    let text = Rope::from_str("foo bar_1 baz\nqux");
    assert_eq!(smart_highlight_token(&text, 4..9, true), Some(4..9));
    assert_eq!(
        smart_highlight_token(&text, 4..7, true),
        None,
        "part of a word"
    );
    assert_eq!(smart_highlight_token(&text, 0..7, true), None, "two words");
    assert_eq!(smart_highlight_token(&text, 4..7, false), Some(4..7));
    assert_eq!(smart_highlight_token(&text, 0..7, false), Some(0..7));
    assert_eq!(
        smart_highlight_token(&text, 10..15, false),
        None,
        "two lines"
    );
    assert_eq!(smart_highlight_token(&text, 4..4, false), None);
}

#[test]
fn the_token_is_the_selection_or_the_word_at_the_caret() {
    let text = Rope::from_str("foo bar, baz");
    assert_eq!(token_at(&text, 1..6), Some(1..6));
    assert_eq!(token_at(&text, 5..5), Some(4..7));
    assert_eq!(token_at(&text, 7..7), Some(4..7), "right after a word");
    assert_eq!(
        token_at(&text, 8..8),
        None,
        "between punctuation and a space"
    );
}
