use birchpad_tools::spell::*;
use std::path::Path;

/// A tiny dictionary: "walk" with its -s, -ed and -ing forms, "the", "cat", "don't".
const AFF: &str = "SET UTF-8\nTRY esianrtolcdugmphbyfvkwz'\n\nSFX S Y 1\nSFX S 0 s .\n\nSFX D Y 1\nSFX D 0 ed .\n\nSFX G Y 1\nSFX G 0 ing .\n";
const DIC: &str = "5\nwalk/SDG\nthe\ncat/S\ndon't\nмир\n";

fn speller() -> Speller {
    Speller::new(AFF, DIC).unwrap()
}

fn found(text: &str) -> Vec<&str> {
    words(text).map(|range| &text[range]).collect()
}

#[test]
fn checks_words_and_their_forms() {
    let speller = speller();
    for word in [
        "walk", "walks", "walked", "walking", "cats", "The", "don't", "мир",
    ] {
        assert!(speller.check(word), "{word}");
    }
    for word in ["wlak", "dog", "catss", "walkd"] {
        assert!(!speller.check(word), "{word}");
    }
}

#[test]
fn suggests_close_words() {
    let speller = speller();
    assert!(speller.suggest("wlak").contains(&"walk".to_owned()));
    assert!(speller.suggest("zzzzzzzzzz").is_empty());
}

#[test]
fn finds_misspelled_words_by_range() {
    let speller = speller();
    let text = "The cat wlaks, the dgo walked.";
    let wrong: Vec<&str> = speller
        .misspelled(text)
        .into_iter()
        .map(|range| &text[range])
        .collect();
    assert_eq!(wrong, ["wlaks", "dgo"]);
    assert!(speller.misspelled("").is_empty());
}

#[test]
fn takes_added_words() {
    let mut speller = speller();
    assert!(!speller.check("Birchpad"));
    speller.add("Birchpad");
    speller.add("  ");
    speller.add("gpui");
    assert!(speller.check("Birchpad"));
    assert!(speller.check("gpui"));
    assert!(speller.check("GPUI".to_lowercase().as_str()));
}

#[test]
fn leaves_code_and_addresses_alone() {
    assert_eq!(
        found("don't stop l'eau ’tis rock'n'roll it's'"),
        ["don't", "stop", "l'eau", "tis", "rock'n'roll", "it's"]
    );
    assert_eq!(
        found("NASA camelCase snake_case x2 a I ok go2 HTTP"),
        ["ok"]
    );
    assert_eq!(
        found("see https://example.com/wrongg mail me@hostt.org or www.sitte.com now"),
        ["see", "mail", "or", "now"]
    );
    assert_eq!(found(""), Vec::<&str>::new());
    assert_eq!(found("  ...  "), Vec::<&str>::new());
    assert_eq!(found("Мир, привет!"), ["Мир", "привет"]);
    // A combining accent stays in its word.
    assert_eq!(found("cafe\u{301} x"), ["cafe\u{301}"]);
}

#[test]
fn refuses_a_broken_dictionary() {
    assert!(matches!(
        Speller::new("FLAG long\nSFX AB Y 1\nSFX AB 0 s .\n", "1\nword/A\n"),
        Err(SpellError::Parse(_))
    ));
    let missing = Path::new("no/such.aff");
    assert!(matches!(
        Speller::open(missing, missing),
        Err(SpellError::Read { .. })
    ));
}

#[test]
fn finds_dictionaries_in_folders() {
    let dir = std::env::temp_dir().join(format!("birchpad-spell-{}", std::process::id()));
    let first = dir.join("first");
    let second = dir.join("second");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    std::fs::write(first.join("en_US.dic"), DIC).unwrap();
    std::fs::write(second.join("en_US.aff"), AFF).unwrap();
    std::fs::write(second.join("en_US.dic"), DIC).unwrap();
    std::fs::write(second.join("de_DE.aff"), AFF).unwrap();
    std::fs::write(second.join("de_DE.dic"), DIC).unwrap();
    std::fs::write(second.join("notes.txt"), "").unwrap();
    let folders = vec![dir.join("missing"), first.clone(), second.clone()];
    let (aff, dic) = find_dictionary(&folders, "en_US").unwrap();
    assert_eq!(aff, second.join("en_US.aff"));
    let speller = Speller::open(&aff, &dic).unwrap();
    assert!(speller.check("walking"));
    assert_eq!(find_dictionary(&folders, "fr_FR"), None);
    assert_eq!(languages(&folders), ["de_DE", "en_US"]);
    assert!(languages(&[]).is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
    // The system's folders are only places to look.
    let _ = system_folders();
}

fn found_by(text: &str, rules: Rules) -> Vec<&str> {
    words_by(text, rules).map(|range| &text[range]).collect()
}

#[test]
fn rules_say_which_words_are_left_alone() {
    let text = "a Bob NASA camelCase snake_case word2 http://x.org/path me@host plain";
    assert_eq!(found_by(text, Rules::default()), found(text));
    assert_eq!(found_by(text, Rules::default()), ["Bob", "plain"]);
    let none = Rules {
        ignore_numbers: false,
        ignore_all_caps: false,
        ignore_capitals_inside: false,
        ignore_capital_start: false,
        ignore_underscores: false,
        ignore_one_letter: false,
        ignore_addresses: false,
    };
    assert_eq!(
        found_by(text, none),
        [
            "a",
            "Bob",
            "NASA",
            "camelCase",
            "snake_case",
            "word2",
            "http",
            "x",
            "org",
            "path",
            "me",
            "host",
            "plain"
        ]
    );
    let names = Rules {
        ignore_capital_start: true,
        ..Rules::default()
    };
    assert_eq!(found_by(text, names), ["plain"]);
    // A word of digits alone is no word, whatever the rules.
    assert!(found_by("123 4_5", none).is_empty());
    assert!(found_by("", none).is_empty());
}

#[test]
fn checks_with_several_dictionaries() {
    let mut speller = speller();
    assert_eq!(speller.dictionaries(), 1);
    assert!(!speller.check("hund"));
    speller
        .add_dictionary("SET UTF-8\n", "2\nhund\nkatze\n")
        .unwrap();
    assert_eq!(speller.dictionaries(), 2);
    assert!(speller.check("hund") && speller.check("walked"));
    assert!(!speller.check("vogel"));
    // Suggestions from both, the first dictionary's first, none twice.
    let suggestions = speller.suggest("hunf");
    assert!(suggestions.contains(&"hund".to_owned()), "{suggestions:?}");
    let unique: std::collections::HashSet<_> = suggestions.iter().collect();
    assert_eq!(unique.len(), suggestions.len());
    // A dictionary that cannot be read is refused, the others kept.
    let missing = Path::new("missing.aff");
    assert!(speller.open_dictionary(missing, missing).is_err());
    assert_eq!(speller.dictionaries(), 2);
}

#[test]
fn the_speller_finds_by_its_rules() {
    let mut speller = speller();
    let text = "Cat wlak NASAA";
    let wrong = |speller: &Speller| -> Vec<String> {
        speller
            .misspelled(text)
            .into_iter()
            .map(|range| text[range].to_owned())
            .collect()
    };
    assert_eq!(wrong(&speller), ["wlak"]);
    speller.set_rules(Rules {
        ignore_all_caps: false,
        ..Rules::default()
    });
    assert!(!speller.rules().ignore_all_caps);
    assert_eq!(wrong(&speller), ["wlak", "NASAA"]);
}
