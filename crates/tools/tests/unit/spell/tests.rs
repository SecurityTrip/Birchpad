use super::*;

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
