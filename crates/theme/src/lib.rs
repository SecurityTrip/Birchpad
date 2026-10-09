//! Color themes (ADR 0027): the colors of the editor, of the rest of the window, and of each
//! kind of highlighted text.
//!
//! A theme file is TOML. It sets any subset of the colors; the rest come from the built-in theme
//! of the same brightness ([`DEFAULT`] or [`DARK`]), so that a few lines make a theme. Notepad++
//! XML themes read too ([`Theme::from_notepad_xml`]).

mod color;
mod notepad;
mod style;

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

pub use color::{Color, ColorError};
pub use style::Style;

/// The name of the built-in light theme, used when no other is chosen.
pub const DEFAULT: &str = "Default";
/// The name of the built-in dark theme.
pub const DARK: &str = "Dark";

/// The highlighting styles of a theme or of one language, by highlight name (`keyword`,
/// `function.method`).
pub type SyntaxStyles = BTreeMap<String, Style>;

/// A field name as theme files write it: `current_line` as `current-line`.
macro_rules! kebab {
    ($field:ident) => {{
        const NAME: &str = stringify!($field);
        const BYTES: [u8; NAME.len()] = {
            let mut bytes = [0; NAME.len()];
            let mut index = 0;
            while index < NAME.len() {
                let byte = NAME.as_bytes()[index];
                bytes[index] = if byte == b'_' { b'-' } else { byte };
                index += 1;
            }
            bytes
        };
        match std::str::from_utf8(&BYTES) {
            Ok(name) => name,
            Err(_) => panic!("field names are ASCII"),
        }
    }};
}

/// Declares a set of colors twice: complete, as the application uses it, and partial, as a file
/// writes it, with every color optional.
macro_rules! palette {
    (
        $(#[$meta:meta])*
        $name:ident / $partial:ident {
            $($(#[$field_meta:meta])* $field:ident,)*
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct $name {
            $($(#[$field_meta])* pub $field: Color,)*
        }

        #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(default, rename_all = "kebab-case")]
        struct $partial {
            $(#[serde(skip_serializing_if = "Option::is_none")] $field: Option<Color>,)*
        }

        impl $name {
            /// The keys of these colors, as theme files write them (`current-line`), in order.
            pub const KEYS: &[&str] = &[$(kebab!($field),)*];

            /// The color of `key` (`current-line`).
            pub fn get(&self, key: &str) -> Option<Color> {
                $(if key == kebab!($field) {
                    return Some(self.$field);
                })*
                None
            }

            /// Sets the color of `key`; false if there is no such key.
            pub fn set(&mut self, key: &str, color: Color) -> bool {
                $(if key == kebab!($field) {
                    self.$field = color;
                    return true;
                })*
                false
            }
        }

        impl $partial {
            /// These colors, the others from `base`.
            fn over(&self, base: &$name) -> $name {
                $name { $($field: self.$field.unwrap_or(base.$field),)* }
            }

            /// The keys this leaves unset.
            fn missing(&self) -> Vec<&'static str> {
                let mut missing = Vec::new();
                $(if self.$field.is_none() {
                    missing.push(stringify!($field));
                })*
                missing
            }
        }

        impl From<&$name> for $partial {
            fn from(colors: &$name) -> Self {
                Self { $($field: Some(colors.$field),)* }
            }
        }
    };
}

palette! {
    /// The colors of the text area and its margins, as Notepad++'s Style Configurator lists
    /// them under "Global Styles".
    EditorColors / PartialEditorColors {
        /// Text without a highlighting style ("Default Style").
        text,
        background,
        /// Behind selected text ("Selected text colour").
        selection,
        caret,
        /// The current line's background or frame ("Current line background colour").
        current_line,
        /// The margins: line numbers, bookmarks, change history, folding.
        gutter_background,
        gutter_text,
        /// A thin line between the margins and the text.
        gutter_border,
        bookmark,
        /// Change history: a line that differs from the saved file, and one changed and saved.
        change_modified,
        change_saved,
        /// The border and sign of fold boxes, and the lines of expanded folds.
        fold_mark,
        fold_line,
        /// The line under a collapsed fold's header.
        fold_underline,
        /// View > Show Symbol: spaces, tabs and the wrap symbol ("White space symbol").
        whitespace,
        /// View > Show End of Line: the boxes and their labels.
        eol_box,
        eol_text,
        /// Indentation guides ("Indent guideline style").
        indent_guide,
        /// The vertical edge ("Edge colour").
        edge,
        /// A bracket at the caret and its partner, drawn bold ("Brace highlight style").
        brace_match,
        /// A bracket without a partner ("Bad brace colour").
        brace_bad,
        /// Smart highlighting of the selected word, translucent.
        smart_highlight,
        /// The matches of Search > Mark ("Find Mark Style"), translucent.
        find_mark,
        /// Incremental search's Highlight all, translucent.
        incremental_highlight,
        /// Search > Style All Occurrences of Token, styles 1 to 5, translucent.
        mark_1,
        mark_2,
        mark_3,
        mark_4,
        mark_5,
        scrollbar_thumb,
    }
}

impl EditorColors {
    /// The five token styles of Search > Style All Occurrences of Token.
    pub fn mark_styles(&self) -> [Color; 5] {
        [
            self.mark_1,
            self.mark_2,
            self.mark_3,
            self.mark_4,
            self.mark_5,
        ]
    }
}

palette! {
    /// The colors of everything around the text: tabs, panels, bars, dialogs.
    UiColors / PartialUiColors {
        /// Panels, lists and tabs.
        background,
        /// Bars and headings: the status bar, the find panel, panel titles.
        surface,
        border,
        text,
        /// Secondary text: paths, counts, hints.
        muted,
        /// Text that is barely there: a tab's close button, disabled entries.
        faint,
        /// The selected row of a list, and where a dragged tab would go.
        selected,
        hovered,
        /// The active view, and drop zones.
        accent,
        /// Error messages.
        error,
        /// Behind a search that found nothing.
        error_background,
        /// The mark of a document with unsaved changes.
        modified,
        /// Folder names in file trees.
        directory,
        /// A heading in the search results.
        heading,
        /// Line numbers in the search results.
        line_number,
        /// The matched text in the search results.
        match_background,
    }
}

/// A complete theme: every color the application draws with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    /// The name in menus: the file name without its extension.
    pub name: String,
    /// Dark themes get the dark versions of standard controls (buttons, inputs, scrollbars).
    pub dark: bool,
    pub editor: EditorColors,
    pub ui: UiColors,
    /// Highlighting styles for every language.
    pub syntax: SyntaxStyles,
    /// Styles of one language that differ from [`Self::syntax`], by language id.
    pub languages: BTreeMap<String, SyntaxStyles>,
}

/// Why a theme did not read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ThemeError {
    #[error("not a theme: {0}")]
    Toml(String),
    #[error("not a Notepad++ theme: {0}")]
    Xml(String),
}

/// A theme file as written: any subset of the colors.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
struct ThemeFile {
    /// Unset: dark when the editor's background is.
    #[serde(skip_serializing_if = "Option::is_none")]
    dark: Option<bool>,
    editor: PartialEditorColors,
    ui: PartialUiColors,
    #[serde(deserialize_with = "read_syntax")]
    syntax: SyntaxStyles,
    #[serde(
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "read_languages"
    )]
    language: BTreeMap<String, SyntaxStyles>,
}

/// The keys of a style table; other keys of a table under `[syntax]` are longer highlight names,
/// so that `function.method = "#000080"` without quotes, which TOML reads as a table `function`
/// with a key `method`, still styles `function.method`.
const STYLE_KEYS: [&str; 5] = ["color", "background", "bold", "italic", "underline"];

fn read_syntax<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<SyntaxStyles, D::Error> {
    let table = toml::Table::deserialize(deserializer)?;
    let mut styles = SyntaxStyles::new();
    flatten("", table, &mut styles).map_err(serde::de::Error::custom)?;
    Ok(styles)
}

fn read_languages<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, SyntaxStyles>, D::Error> {
    let tables = BTreeMap::<String, toml::Table>::deserialize(deserializer)?;
    let mut languages = BTreeMap::new();
    for (language, table) in tables {
        let mut styles = SyntaxStyles::new();
        flatten("", table, &mut styles)
            .map_err(|error| serde::de::Error::custom(format!("language.{language}: {error}")))?;
        languages.insert(language, styles);
    }
    Ok(languages)
}

/// Adds the styles of `table`, whose names start with `prefix`, to `styles`.
fn flatten(prefix: &str, table: toml::Table, styles: &mut SyntaxStyles) -> Result<(), String> {
    for (key, value) in table {
        let name = if prefix.is_empty() {
            key
        } else {
            format!("{prefix}.{key}")
        };
        let style = match value {
            toml::Value::Table(table) => {
                let (own, children): (toml::Table, toml::Table) = table
                    .into_iter()
                    .partition(|(key, _)| STYLE_KEYS.contains(&key.as_str()));
                // Longer names are colors or tables; other values are style keys of a newer
                // version.
                let children: toml::Table = children
                    .into_iter()
                    .filter(|(_, value)| {
                        matches!(value, toml::Value::String(_) | toml::Value::Table(_))
                    })
                    .collect();
                // A table of longer names only does not style its own name; an empty table is
                // a style without colors.
                let namespace = own.is_empty() && !children.is_empty();
                flatten(&name, children, styles)?;
                if namespace {
                    continue;
                }
                toml::Value::Table(own)
            }
            value => value,
        };
        let style =
            Style::deserialize(style).map_err(|error| format!("{name}: {}", error.message()))?;
        styles.insert(name, style);
    }
    Ok(())
}

impl ThemeFile {
    fn is_dark(&self) -> bool {
        self.dark
            .unwrap_or_else(|| self.editor.background.is_some_and(Color::is_dark))
    }

    /// The theme this file makes over the built-in theme of its brightness.
    fn resolve(self, name: &str) -> Theme {
        let dark = self.is_dark();
        let base =
            Theme::builtin(if dark { DARK } else { DEFAULT }).expect("the built-in themes exist");
        self.over(name, dark, base)
    }

    fn over(self, name: &str, dark: bool, base: &Theme) -> Theme {
        let mut languages = base.languages.clone();
        for (language, styles) in self.language {
            let language = language.to_ascii_lowercase();
            let merged = merge(languages.remove(&language).unwrap_or_default(), styles);
            languages.insert(language, merged);
        }
        Theme {
            name: name.to_owned(),
            dark,
            editor: self.editor.over(&base.editor),
            ui: self.ui.over(&base.ui),
            syntax: merge(base.syntax.clone(), self.syntax),
            languages,
        }
    }
}

/// `styles` over `base`. A style for `function` replaces the base's `function.method` too, so
/// that what a file sets is not hidden by a more specific style it does not know about.
fn merge(mut base: SyntaxStyles, styles: SyntaxStyles) -> SyntaxStyles {
    base.retain(|name, _| {
        !styles.keys().any(|set| {
            name == set
                || name
                    .strip_prefix(set.as_str())
                    .is_some_and(|rest| rest.starts_with('.'))
        })
    });
    base.extend(styles);
    base
}

/// The built-in themes, each a complete theme file.
const BUILTIN: [(&str, &str); 2] = [
    (DEFAULT, include_str!("themes/default.toml")),
    (DARK, include_str!("themes/dark.toml")),
];

impl Theme {
    /// Every built-in theme: [`DEFAULT`], then [`DARK`].
    pub fn builtins() -> &'static [Theme] {
        static THEMES: OnceLock<Vec<Theme>> = OnceLock::new();
        THEMES.get_or_init(|| {
            BUILTIN
                .iter()
                .map(|(name, text)| {
                    let file: ThemeFile = toml::from_str(text)
                        .unwrap_or_else(|error| panic!("built-in theme {name}: {error}"));
                    let dark = file.is_dark();
                    // Complete files need no base; an empty one shows what is missing.
                    let missing: Vec<_> = [file.editor.missing(), file.ui.missing()].concat();
                    assert!(
                        missing.is_empty(),
                        "built-in theme {name} lacks {missing:?}"
                    );
                    file.over(name, dark, &Theme::blank())
                })
                .collect()
        })
    }

    /// The built-in theme of this name, ignoring case.
    pub fn builtin(name: &str) -> Option<&'static Theme> {
        Self::builtins()
            .iter()
            .find(|theme| theme.name.eq_ignore_ascii_case(name))
    }

    /// The built-in light theme.
    pub fn default_theme() -> &'static Theme {
        &Self::builtins()[0]
    }

    /// A theme with every color black, which complete files are read over.
    fn blank() -> Theme {
        let black = Color::rgb(0);
        Theme {
            name: String::new(),
            dark: false,
            editor: EditorColors {
                text: black,
                background: black,
                selection: black,
                caret: black,
                current_line: black,
                gutter_background: black,
                gutter_text: black,
                gutter_border: black,
                bookmark: black,
                change_modified: black,
                change_saved: black,
                fold_mark: black,
                fold_line: black,
                fold_underline: black,
                whitespace: black,
                eol_box: black,
                eol_text: black,
                indent_guide: black,
                edge: black,
                brace_match: black,
                brace_bad: black,
                smart_highlight: black,
                find_mark: black,
                incremental_highlight: black,
                mark_1: black,
                mark_2: black,
                mark_3: black,
                mark_4: black,
                mark_5: black,
                scrollbar_thumb: black,
            },
            ui: UiColors {
                background: black,
                surface: black,
                border: black,
                text: black,
                muted: black,
                faint: black,
                selected: black,
                hovered: black,
                accent: black,
                error: black,
                error_background: black,
                modified: black,
                directory: black,
                heading: black,
                line_number: black,
                match_background: black,
            },
            syntax: SyntaxStyles::new(),
            languages: BTreeMap::new(),
        }
    }

    /// Reads a theme file named `name`. Colors it does not set come from the built-in theme of
    /// its brightness.
    pub fn from_toml(name: &str, text: &str) -> Result<Self, ThemeError> {
        let file: ThemeFile =
            toml::from_str(text).map_err(|error| ThemeError::Toml(error.message().to_owned()))?;
        Ok(file.resolve(name))
    }

    /// The theme as a complete file, which [`Self::from_toml`] reads back as the same theme.
    pub fn to_toml(&self) -> String {
        let file = ThemeFile {
            dark: Some(self.dark),
            editor: (&self.editor).into(),
            ui: (&self.ui).into(),
            syntax: self.syntax.clone(),
            language: self.languages.clone(),
        };
        toml::to_string(&file).expect("a theme always serializes")
    }

    /// Reads a Notepad++ theme (its `stylers.xml` or a file of its `themes` folder).
    pub fn from_notepad_xml(name: &str, xml: &str) -> Result<Self, ThemeError> {
        notepad::import(xml).map(|file| file.resolve(name))
    }

    /// The style of the highlight `name` (`function.method`) in `language`. Names fall back to
    /// their prefix (`function`); at each step, the language's own style comes first.
    pub fn syntax_style(&self, language: Option<&str>, name: &str) -> Option<Style> {
        let own = language.and_then(|language| self.languages.get(language));
        let mut name = name;
        loop {
            if let Some(style) = own.and_then(|styles| styles.get(name)) {
                return Some(*style);
            }
            if let Some(style) = self.syntax.get(name) {
                return Some(*style);
            }
            name = &name[..name.rfind('.')?];
        }
    }
}

#[cfg(test)]
mod tests;
