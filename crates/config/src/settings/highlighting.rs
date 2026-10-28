//! Preferences > Highlighting: smart highlighting and matching tags.

use serde::{Deserialize, Serialize};

/// Notepad++'s Preferences > Highlighting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct HighlightingSettings {
    /// Highlighting every occurrence of the selected word in the visible text.
    pub smart: SmartHighlighting,
    /// How Search > Style All Occurrences of Token matches the token.
    pub token_style: TokenMatching,
    /// Smart highlighting marks the word in the other view's document too.
    pub smart_other_view: bool,
    /// The partner of the HTML or XML tag at the caret is highlighted, with it ("Highlight
    /// Matching Tags"), their attributes too, and inside comments as well.
    pub matching_tags: bool,
    pub matching_tag_attributes: bool,
    pub matching_tags_in_comments: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct SmartHighlighting {
    pub enabled: bool,
    pub match_case: bool,
    /// Highlight only when a whole word is selected, and only whole-word occurrences of it.
    /// Off: any selection on one line is highlighted wherever it occurs.
    pub whole_word: bool,
    /// Take match case and whole word from the find panel instead.
    pub use_find_options: bool,
}

impl Default for HighlightingSettings {
    fn default() -> Self {
        Self {
            smart: SmartHighlighting::default(),
            token_style: TokenMatching::default(),
            smart_other_view: false,
            matching_tags: true,
            matching_tag_attributes: false,
            matching_tags_in_comments: false,
        }
    }
}

impl Default for SmartHighlighting {
    fn default() -> Self {
        Self {
            enabled: true,
            match_case: false,
            whole_word: true,
            use_find_options: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct TokenMatching {
    pub match_case: bool,
    pub whole_word: bool,
}

impl Default for TokenMatching {
    fn default() -> Self {
        Self {
            match_case: false,
            whole_word: true,
        }
    }
}
