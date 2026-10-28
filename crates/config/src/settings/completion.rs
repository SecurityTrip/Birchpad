//! Preferences > Auto-Completion.

use serde::{Deserialize, Serialize};

/// Notepad++'s Preferences > Auto-Completion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct CompletionSettings {
    /// Brackets and quotes typed in pairs, written as pairs of characters: `()[]{}""''`. Typing the
    /// closing one steps over it; Backspace between an empty pair deletes both.
    pub auto_close: String,
    /// Show the words of the document while typing ("Word completion").
    pub words: bool,
    /// From how many characters of a word the document's words show.
    pub min_chars: u8,
    /// Show the language's API keywords and functions while typing ("Function completion").
    pub api: bool,
    /// From how many characters of a word the API's keywords show.
    pub api_min_chars: u8,
    /// Words and keywords match what is typed without regard to case.
    pub ignore_case: bool,
    /// Numbers are not offered as words.
    pub ignore_numbers: bool,
    /// Show a function's parameters after its opening bracket, from the language's API file.
    pub call_tips: bool,
    /// Typing the `>` of an HTML or XML opening tag inserts its closing tag after the caret.
    pub close_tags: bool,
    /// The keys that take the entry chosen in the list ("Insert Selection").
    pub accept_with_tab: bool,
    pub accept_with_enter: bool,
    /// The list holds only the entries that start with what is typed, case and all, and
    /// shrinks as typing goes on ("Make auto-completion list brief").
    pub brief: bool,
}

impl Default for CompletionSettings {
    fn default() -> Self {
        Self {
            auto_close: "()[]{}\"\"''".to_owned(),
            words: true,
            min_chars: 1,
            api: true,
            api_min_chars: 1,
            ignore_case: true,
            ignore_numbers: true,
            call_tips: true,
            close_tags: false,
            accept_with_tab: true,
            accept_with_enter: true,
            brief: false,
        }
    }
}

impl CompletionSettings {
    /// The pairs of `auto_close`, two characters each; an odd character at the end is left out.
    pub fn pairs(&self) -> Vec<(char, char)> {
        let chars: Vec<char> = self.auto_close.chars().collect();
        let (pairs, _) = chars.as_chunks::<2>();
        pairs.iter().map(|[open, close]| (*open, *close)).collect()
    }
}
