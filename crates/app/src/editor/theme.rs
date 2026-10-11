//! The active color theme (ADR 0027) and how decorations are drawn.
//!
//! The theme is kept per thread: the UI runs on one, and each test on its own, so that a test
//! that switches themes does not repaint another's windows.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use birchpad_theme::{Color, EditorColors, Theme, UiColors};
use gpui_kit::{Rgba, rgba};

/// How a decoration range is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Paint {
    /// A translucent background behind the text.
    Fill(Rgba),
    /// A rectangle around the text.
    #[expect(dead_code, reason = "used by later decorations")]
    Outline(Rgba),
    /// A line under the text.
    #[expect(dead_code, reason = "used by later decorations")]
    Underline(Rgba),
}

/// How highlighted text looks. Colors are `0xRRGGBBAA`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TextStyle {
    /// Unset: the theme's text color.
    pub(crate) color: Option<u32>,
    pub(crate) background: Option<u32>,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) underline: bool,
}

impl From<birchpad_theme::Style> for TextStyle {
    fn from(style: birchpad_theme::Style) -> Self {
        Self {
            color: style.color.map(|color| color.0),
            background: style.background.map(|color| color.0),
            bold: style.bold,
            italic: style.italic,
            underline: style.underline,
        }
    }
}

/// A theme in use, with its highlighting styles looked up once per language.
struct Active {
    theme: Rc<Theme>,
    /// Bumped by every [`set`], so that what was colored with the last theme is colored again.
    generation: u64,
    /// For each language id (`""` for none), the style of each highlight.
    tables: RefCell<HashMap<&'static str, Rc<[Option<TextStyle>]>>>,
}

thread_local! {
    static ACTIVE: RefCell<Rc<Active>> = RefCell::new(Rc::new(Active {
        theme: Rc::new(Theme::default_theme().clone()),
        generation: 0,
        tables: RefCell::default(),
    }));
}

fn active() -> Rc<Active> {
    ACTIVE.with(|active| active.borrow().clone())
}

/// Makes `theme` the one everything is drawn with. Windows repaint on their next frame.
pub(crate) fn set(theme: Theme) {
    ACTIVE.with(|active| {
        let generation = active.borrow().generation + 1;
        *active.borrow_mut() = Rc::new(Active {
            theme: Rc::new(theme),
            generation,
            tables: RefCell::default(),
        });
    });
}

/// The theme in use.
pub(crate) fn current() -> Rc<Theme> {
    active().theme.clone()
}

/// Changes with every [`set`].
pub(crate) fn generation() -> u64 {
    active().generation
}

pub(crate) fn editor() -> EditorColors {
    active().theme.editor
}

pub(crate) fn ui() -> UiColors {
    active().theme.ui
}

/// A theme color as GPUI paints it.
pub(crate) fn paint(color: Color) -> Rgba {
    rgba(color.0)
}

/// Search > Style All Occurrences of Token, styles 1 to 5.
pub(crate) fn mark_style(index: usize) -> Paint {
    Paint::Fill(paint(editor().mark_styles()[index]))
}

/// A bracket at the caret and its partner.
pub(crate) fn brace_match() -> TextStyle {
    brace(editor().brace_match)
}

/// A bracket without a partner.
pub(crate) fn brace_bad() -> TextStyle {
    brace(editor().brace_bad)
}

fn brace(color: Color) -> TextStyle {
    TextStyle {
        color: Some(color.0),
        background: None,
        bold: true,
        italic: false,
        underline: false,
    }
}

/// The style of a syntax highlight in a document of `language`, if it has one.
pub(crate) fn syntax_style(
    language: Option<&'static str>,
    highlight: birchpad_syntax::Highlight,
) -> Option<TextStyle> {
    let active = active();
    let key = language.unwrap_or("");
    let table = active.tables.borrow().get(key).cloned();
    let table = table.unwrap_or_else(|| {
        let table = table_for(&active.theme, language);
        active.tables.borrow_mut().insert(key, table.clone());
        table
    });
    table[usize::from(highlight.0)]
}

fn table_for(theme: &Theme, language: Option<&str>) -> Rc<[Option<TextStyle>]> {
    // A theme names languages by id, or as Notepad++ does (`cs`, `javascript.js`).
    let own = language.and_then(|id| {
        theme.languages.keys().find(|name| {
            name.as_str() == id
                || birchpad_syntax::injected(name).is_some_and(|found| found.id == id)
        })
    });
    birchpad_syntax::HIGHLIGHT_NAMES
        .iter()
        .map(|name| {
            theme
                .syntax_style(own.map(String::as_str), name)
                .map(TextStyle::from)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use birchpad_syntax::Highlight;

    #[test]
    fn styles_follow_the_theme_and_its_languages() {
        let keyword = Highlight::named("keyword").unwrap();
        let default = syntax_style(Some("python"), keyword).unwrap();
        assert_eq!(default.color, Some(0x0000ffff));
        assert!(default.bold);
        let before = generation();
        set(Theme::from_toml(
            "Mine",
            "[syntax]\nkeyword = '#111111'\n[language.cs]\nkeyword = { color = '#222222', italic = true }\n",
        )
        .unwrap());
        assert_ne!(generation(), before);
        assert_eq!(syntax_style(None, keyword).unwrap().color, Some(0x111111ff));
        assert_eq!(
            syntax_style(Some("python"), keyword).unwrap().color,
            Some(0x111111ff)
        );
        // Found by the Notepad++ name of the language.
        let cs = syntax_style(birchpad_syntax::by_id("cs").map(|l| l.id), keyword).unwrap();
        assert_eq!(cs.color, Some(0x222222ff));
        assert!(cs.italic);
        // A highlight without a style, and a language without styles of its own.
        assert_eq!(
            syntax_style(Some("cs"), Highlight::named("variable").unwrap()),
            None
        );
        assert_eq!(current().name, "Mine");
        set(Theme::default_theme().clone());
        assert_eq!(editor(), Theme::default_theme().editor);
        assert_eq!(ui(), Theme::default_theme().ui);
    }

    #[test]
    fn braces_and_marks() {
        assert_eq!(brace_match().color, Some(0xff0000ff));
        assert!(brace_match().bold && brace_bad().bold);
        assert_eq!(brace_bad().color, Some(0x800000ff));
        assert_eq!(mark_style(0), Paint::Fill(rgba(0x00ffff66)));
        assert_eq!(mark_style(4), Paint::Fill(rgba(0x00800066)));
    }
}
