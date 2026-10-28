//! The tools' settings: the spell checker, the hex editor and Compare.

use serde::{Deserialize, Serialize};

/// Plugins > Spell Checker, as DSpellCheck's settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct SpellSettings {
    /// Languages checked together (`en_US ru_RU`): a word is right if one of them has it.
    /// Empty: the one chosen in Plugins > Spell Checker > Language.
    pub languages: String,
    /// What of a document with a language is checked.
    pub check: SpellScope,
    /// Words with digits are left alone.
    pub ignore_numbers: bool,
    /// Words in capitals (acronyms) are left alone.
    pub ignore_all_caps: bool,
    /// Words with a capital after their first letter (`camelCase`) are left alone.
    pub ignore_capitals_inside: bool,
    /// Words that start with a capital (names) are left alone.
    pub ignore_capital_start: bool,
    /// Words with underscores are left alone.
    pub ignore_underscores: bool,
    /// Single letters are left alone.
    pub ignore_one_letter: bool,
    /// Words in addresses (`https://...`, `name@host`) are left alone.
    pub ignore_addresses: bool,
}

impl Default for SpellSettings {
    fn default() -> Self {
        Self {
            languages: String::new(),
            check: SpellScope::default(),
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

impl SpellSettings {
    /// The languages of `languages`, as written, separated by spaces or commas.
    pub fn language_list(&self) -> Vec<String> {
        self.languages
            .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
            .filter(|language| !language.is_empty())
            .map(str::to_owned)
            .collect()
    }
}

/// What of a document with a language the spell checker reads: its text by syntax style.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpellScope {
    #[default]
    CommentsAndStrings,
    Comments,
    Strings,
    /// All of it, code too.
    Everything,
}

/// Plugins > Hex Editor's view of a document's bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct HexSettings {
    /// Bytes a row shows, from 1 to 64.
    pub columns: u8,
    /// Bytes put together without a space between them, from 1 to the columns.
    pub group: u8,
    /// `A` to `F` in capitals.
    pub uppercase: bool,
}

impl Default for HexSettings {
    fn default() -> Self {
        Self {
            columns: 16,
            group: 1,
            uppercase: true,
        }
    }
}

impl HexSettings {
    /// The bytes a row shows and how many make a group, within their limits.
    pub fn layout(&self) -> (usize, usize) {
        let columns = usize::from(self.columns).clamp(1, 64);
        (columns, usize::from(self.group).clamp(1, columns))
    }
}

/// Plugins > Compare, as ComparePlus's settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct CompareSettings {
    /// Blank rows keep the lines of the two documents side by side ("Align matches").
    pub align: bool,
    /// What differs within a changed line is marked too ("Detect character diffs").
    pub inline: bool,
    /// The differences are marked along the vertical scrollbar, where a click goes to them
    /// (ComparePlus's navigation bar).
    pub navigation_bar: bool,
    /// The documents are compared again a moment after either changes ("Auto re-compare").
    pub recompare: bool,
}

impl Default for CompareSettings {
    fn default() -> Self {
        Self {
            align: true,
            inline: true,
            navigation_bar: true,
            recompare: true,
        }
    }
}
