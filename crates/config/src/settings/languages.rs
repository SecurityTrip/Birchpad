//! Preferences > Language: the Language menu, tab settings by language, SQL.

use serde::{Deserialize, Serialize};

/// How SQL documents are read: Notepad++'s Preferences > Language > SQL.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct SqlSettings {
    /// A backslash escapes the quote after it in strings (`'it\'s'`), as MySQL reads them;
    /// off, as standard SQL does, only a doubled quote does ("Treat backslash as escape
    /// character for SQL").
    pub backslash_escapes: bool,
}

/// Notepad++'s Preferences > Language: the Language menu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct LanguageMenuSettings {
    /// The languages in submenus by their first letter ("Make language menu compact"); off,
    /// one under the other.
    pub compact: bool,
    /// Ids of languages the menu leaves out, separated by spaces (`cobol fortran`).
    pub hidden: String,
}

impl Default for LanguageMenuSettings {
    fn default() -> Self {
        Self {
            compact: true,
            hidden: String::new(),
        }
    }
}

impl LanguageMenuSettings {
    /// Whether the menu lists the language `id`.
    pub fn shows(&self, id: &str) -> bool {
        !self
            .hidden
            .split([' ', ',', ';'])
            .filter(|hidden| !hidden.is_empty())
            .any(|hidden| hidden.eq_ignore_ascii_case(id))
    }
}

/// One language's tab settings; unset ones are `editor`'s.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct LanguageSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab_width: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert_spaces: Option<bool>,
}
