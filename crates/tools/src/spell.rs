//! Spell checking with Hunspell dictionaries (`.aff` and `.dic`), as Notepad++'s DSpellCheck
//! plugin does, through `spellbook`.
//!
//! Only words are checked: runs of letters, with apostrophes inside. By default words with
//! digits or underscores, words in capitals (acronyms), words with a capital inside
//! (`camelCase`), single letters, and anything in an address (`https://...`, `name@host`) are
//! left alone, as they are mostly code and names; [`Rules`] says which of these are. A speller
//! may hold several dictionaries: a word is right if one of them has it.

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

/// Which words are left alone, as DSpellCheck's options say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rules {
    pub ignore_numbers: bool,
    pub ignore_all_caps: bool,
    pub ignore_capitals_inside: bool,
    pub ignore_capital_start: bool,
    pub ignore_underscores: bool,
    pub ignore_one_letter: bool,
    pub ignore_addresses: bool,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            ignore_numbers: true,
            ignore_all_caps: true,
            ignore_capitals_inside: true,
            ignore_capital_start: false,
            ignore_underscores: true,
            ignore_one_letter: true,
            ignore_addresses: true,
        }
    }
}

/// Dictionaries and the words the user added to them.
pub struct Speller {
    dictionaries: Vec<spellbook::Dictionary>,
    /// Words added by the user, checked as they are written and in lower case.
    personal: HashSet<String>,
    rules: Rules,
}

fn read_file(path: &Path) -> Result<String, SpellError> {
    std::fs::read(path)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .map_err(|error| SpellError::Read {
            path: path.to_owned(),
            error,
        })
}

impl Speller {
    /// A speller from the text of an `.aff` file and of its `.dic` file.
    pub fn new(aff: &str, dic: &str) -> Result<Self, SpellError> {
        let dictionary = spellbook::Dictionary::new(aff, dic)
            .map_err(|error| SpellError::Parse(error.to_string()))?;
        Ok(Self {
            dictionaries: vec![dictionary],
            personal: HashSet::new(),
            rules: Rules::default(),
        })
    }

    /// A speller from `<name>.aff` and `<name>.dic`.
    pub fn open(aff: &Path, dic: &Path) -> Result<Self, SpellError> {
        Self::new(&read_file(aff)?, &read_file(dic)?)
    }

    /// Checks with another dictionary too, from the text of its `.aff` and `.dic` files.
    pub fn add_dictionary(&mut self, aff: &str, dic: &str) -> Result<(), SpellError> {
        let dictionary = spellbook::Dictionary::new(aff, dic)
            .map_err(|error| SpellError::Parse(error.to_string()))?;
        self.dictionaries.push(dictionary);
        Ok(())
    }

    /// Checks with the dictionary of `<name>.aff` and `<name>.dic` too.
    pub fn open_dictionary(&mut self, aff: &Path, dic: &Path) -> Result<(), SpellError> {
        self.add_dictionary(&read_file(aff)?, &read_file(dic)?)
    }

    /// How many dictionaries it checks with.
    pub fn dictionaries(&self) -> usize {
        self.dictionaries.len()
    }

    /// Leaves the words `rules` says alone.
    pub fn set_rules(&mut self, rules: Rules) {
        self.rules = rules;
    }

    pub fn rules(&self) -> Rules {
        self.rules
    }

    /// Whether `word` is spelled right: added by the user, or in one of the dictionaries.
    pub fn check(&self, word: &str) -> bool {
        self.personal.contains(word)
            || self.personal.contains(&word.to_lowercase())
            || self
                .dictionaries
                .iter()
                .any(|dictionary| dictionary.check(word))
    }

    /// Words `word` may have been meant as, the likeliest first: the first dictionary's, then
    /// the others' not given yet.
    pub fn suggest(&self, word: &str) -> Vec<String> {
        let mut all: Vec<String> = Vec::new();
        for dictionary in &self.dictionaries {
            let mut out = Vec::new();
            dictionary.suggest(word, &mut out);
            for suggestion in out {
                if !all.contains(&suggestion) {
                    all.push(suggestion);
                }
            }
        }
        all
    }

    /// Takes `word` as spelled right from now on.
    pub fn add(&mut self, word: &str) {
        let word = word.trim();
        if !word.is_empty() {
            self.personal.insert(word.to_owned());
        }
    }

    /// The words of `text` that are spelled wrong, as byte ranges, by its rules.
    pub fn misspelled(&self, text: &str) -> Vec<Range<usize>> {
        words_by(text, self.rules)
            .filter(|range| !self.check(&text[range.clone()]))
            .collect()
    }
}

/// The words of `text` worth checking by the default rules, as byte ranges.
pub fn words(text: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    words_by(text, Rules::default())
}

/// The words of `text` worth checking by `rules`, as byte ranges.
pub fn words_by(text: &str, rules: Rules) -> impl Iterator<Item = Range<usize>> + '_ {
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
            if worth_checking(text, start..at, rules) {
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

fn worth_checking(text: &str, range: Range<usize>, rules: Rules) -> bool {
    let word = &text[range.clone()];
    let letters: Vec<char> = word.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.is_empty() || (rules.ignore_one_letter && letters.len() < 2) {
        return false;
    }
    if rules.ignore_numbers && word.chars().any(char::is_numeric) {
        return false;
    }
    if rules.ignore_underscores && word.contains('_') {
        return false;
    }
    let all_caps = letters.len() > 1 && letters.iter().all(|c| !c.is_lowercase());
    if rules.ignore_all_caps && all_caps {
        return false;
    }
    let capital_inside = letters[1..].iter().any(|c| c.is_uppercase()) && !all_caps;
    if rules.ignore_capitals_inside && capital_inside {
        return false;
    }
    if rules.ignore_capital_start && letters[0].is_uppercase() {
        return false;
    }
    if !rules.ignore_addresses {
        return true;
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
