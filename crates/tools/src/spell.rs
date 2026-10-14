//! Spell checking with Hunspell dictionaries (`.aff` and `.dic`), as Notepad++'s DSpellCheck
//! plugin does, through `spellbook`.
//!
//! Only words are checked: runs of letters, with apostrophes inside. Words with digits or
//! underscores, words in capitals (acronyms), words with a capital inside (`camelCase`), single
//! letters, and anything in an address (`https://...`, `name@host`) are left alone, as they are
//! mostly code and names.

use std::collections::HashSet;
use std::ops::Range;
use std::path::{Path, PathBuf};

/// A dictionary that does not read.
#[derive(Debug, thiserror::Error)]
pub enum SpellError {
    #[error("{}", birchpad_i18n::t!("tools.spell.cannot-read", path = .path.display(), error = .error))]
    Read {
        path: PathBuf,
        error: std::io::Error,
    },
    #[error("{}", birchpad_i18n::t!("tools.spell.does-not-read", reason = .0))]
    Parse(String),
}

/// A dictionary and the words the user added to it.
pub struct Speller {
    dictionary: spellbook::Dictionary,
    /// Words added by the user, checked as they are written and in lower case.
    personal: HashSet<String>,
}

impl Speller {
    /// A speller from the text of an `.aff` file and of its `.dic` file.
    pub fn new(aff: &str, dic: &str) -> Result<Self, SpellError> {
        let dictionary = spellbook::Dictionary::new(aff, dic)
            .map_err(|error| SpellError::Parse(error.to_string()))?;
        Ok(Self {
            dictionary,
            personal: HashSet::new(),
        })
    }

    /// A speller from `<name>.aff` and `<name>.dic`.
    pub fn open(aff: &Path, dic: &Path) -> Result<Self, SpellError> {
        let read = |path: &Path| {
            std::fs::read(path)
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .map_err(|error| SpellError::Read {
                    path: path.to_owned(),
                    error,
                })
        };
        Self::new(&read(aff)?, &read(dic)?)
    }

    /// Whether `word` is spelled right.
    pub fn check(&self, word: &str) -> bool {
        self.personal.contains(word)
            || self.personal.contains(&word.to_lowercase())
            || self.dictionary.check(word)
    }

    /// Words `word` may have been meant as, the likeliest first.
    pub fn suggest(&self, word: &str) -> Vec<String> {
        let mut out = Vec::new();
        self.dictionary.suggest(word, &mut out);
        out
    }

    /// Takes `word` as spelled right from now on.
    pub fn add(&mut self, word: &str) {
        let word = word.trim();
        if !word.is_empty() {
            self.personal.insert(word.to_owned());
        }
    }

    /// The words of `text` that are spelled wrong, as byte ranges.
    pub fn misspelled(&self, text: &str) -> Vec<Range<usize>> {
        words(text)
            .filter(|range| !self.check(&text[range.clone()]))
            .collect()
    }
}

/// The words of `text` worth checking, as byte ranges.
pub fn words(text: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    let mut at = 0;
    std::iter::from_fn(move || {
        while at < text.len() {
            let rest = &text[at..];
            let skip = rest
                .char_indices()
                .find(|(_, c)| is_word_char(*c))
                .map_or(rest.len(), |(offset, _)| offset);
            at += skip;
            if at >= text.len() {
                return None;
            }
            let start = at;
            let rest = &text[start..];
            let mut end = rest.len();
            let mut chars = rest.char_indices().peekable();
            while let Some((offset, c)) = chars.next() {
                if is_word_char(c) {
                    continue;
                }
                // An apostrophe between letters belongs to the word: don't, l'eau.
                let inside = matches!(c, '\'' | '\u{2019}')
                    && offset > 0
                    && chars.peek().is_some_and(|(_, next)| next.is_alphabetic());
                if !inside {
                    end = offset;
                    break;
                }
            }
            at = start + end;
            if worth_checking(text, start..at) {
                return Some(start..at);
            }
        }
        None
    })
}

/// Letters, digits, underscores, and combining accents typed after their letter.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || ('\u{0300}'..='\u{036F}').contains(&c)
}

fn worth_checking(text: &str, range: Range<usize>) -> bool {
    let word = &text[range.clone()];
    let letters = word.chars().filter(|c| c.is_alphabetic()).count();
    if letters < 2 || word.chars().any(|c| c.is_numeric() || c == '_') {
        return false;
    }
    let mut chars = word.chars().filter(|c| c.is_alphabetic());
    let rest_upper = chars.next().is_some() && chars.clone().any(char::is_uppercase);
    if rest_upper {
        // ALLCAPS or camelCase.
        return false;
    }
    // Part of an address: the run of non-blank text around it holds "://", "@" or "www.".
    let start = text[..range.start]
        .rfind(char::is_whitespace)
        .map_or(0, |at| at + 1);
    let end = text[range.end..]
        .find(char::is_whitespace)
        .map_or(text.len(), |at| range.end + at);
    let around = &text[start..end];
    !(around.contains("://") || around.contains('@') || around.contains("www."))
}

/// The `.aff` and `.dic` files of `language` (`en_US`) in the first of `folders` that has
/// them.
pub fn find_dictionary(folders: &[PathBuf], language: &str) -> Option<(PathBuf, PathBuf)> {
    folders.iter().find_map(|folder| {
        let aff = folder.join(format!("{language}.aff"));
        let dic = folder.join(format!("{language}.dic"));
        (aff.is_file() && dic.is_file()).then_some((aff, dic))
    })
}

/// The languages (`en_US`, `ru_RU`) of the dictionaries in `folders`, sorted, without repeats.
pub fn languages(folders: &[PathBuf]) -> Vec<String> {
    let mut found: Vec<String> = folders
        .iter()
        .filter_map(|folder| std::fs::read_dir(folder).ok())
        .flatten()
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension()? != "dic" || !path.with_extension("aff").is_file() {
                return None;
            }
            Some(path.file_stem()?.to_str()?.to_owned())
        })
        .collect();
    found.sort();
    found.dedup();
    found
}

/// Where the system keeps Hunspell dictionaries.
pub fn system_folders() -> Vec<PathBuf> {
    let mut folders = Vec::new();
    if cfg!(target_os = "linux") {
        folders.push(PathBuf::from("/usr/share/hunspell"));
        folders.push(PathBuf::from("/usr/share/myspell/dicts"));
        folders.push(PathBuf::from("/usr/share/myspell"));
    }
    if cfg!(target_os = "macos") {
        folders.push(PathBuf::from("/Library/Spelling"));
        if let Some(home) = std::env::var_os("HOME") {
            folders.push(PathBuf::from(home).join("Library/Spelling"));
        }
    }
    if cfg!(windows)
        && let Some(programs) = std::env::var_os("ProgramFiles")
    {
        // Where Notepad++'s DSpellCheck keeps the dictionaries it downloads.
        folders.push(PathBuf::from(programs).join("Notepad++/plugins/config/Hunspell"));
    }
    folders
}

#[cfg(test)]
mod tests {
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
}
