//! Colors of the editor and how decorations are drawn.
//!
//! Values follow Notepad++'s default style (stylers.xml) where it has one. Phase 4 replaces
//! these constants with a theme loaded from settings.

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

/// Search > Style All Occurrences of Token, styles 1 to 5 (cyan, orange, yellow, purple,
/// green), translucent like Notepad++'s round boxes.
pub(crate) const MARK_STYLES: [u32; crate::buffer::MARK_STYLES] =
    [0x00ffff66, 0xff800066, 0xffff0099, 0x8000ff55, 0x00800066];

pub(crate) fn mark_style(index: usize) -> Paint {
    Paint::Fill(rgba(MARK_STYLES[index]))
}

/// The matches of Search > Mark: Notepad++'s "Find Mark Style", a translucent red.
pub(crate) const FIND_MARK: u32 = 0xff000055;
/// Notepad++'s "Incremental highlight all", translucent.
pub(crate) const INCREMENTAL_HIGHLIGHT: u32 = 0x0080ff55;

/// Smart highlighting: Notepad++'s translucent green.
pub(crate) const SMART_HIGHLIGHT: u32 = 0x00ff0064;

/// The bookmark symbol in the symbol margin.
pub(crate) const BOOKMARK: u32 = 0x2f6fde;
/// Change history, in Notepad++'s colors: a line that differs from the saved file, and one
/// changed and saved.
pub(crate) const CHANGE_MODIFIED: u32 = 0xff8000;
pub(crate) const CHANGE_SAVED: u32 = 0x00a000;
pub(crate) const GUTTER_BACKGROUND: u32 = 0xf6f8fa;
pub(crate) const GUTTER_TEXT: u32 = 0x8c959f;
/// A thin line between the margins and the text.
pub(crate) const GUTTER_BORDER: u32 = 0xe4e7eb;
/// The border and sign of fold boxes.
pub(crate) const FOLD_MARK: u32 = 0x6e7781;
/// The lines of expanded folds in the folding margin.
pub(crate) const FOLD_LINE: u32 = 0xc4c9cf;
/// The line under a collapsed fold's header.
pub(crate) const FOLD_UNDERLINE: u32 = 0x9aa1a9;

/// View > Show Symbol: dots for spaces, arrows for tabs and the wrap symbol, in Notepad++'s
/// orange ("White space symbol").
pub(crate) const WHITESPACE: u32 = 0xffb56a;
/// View > Show End of Line: the boxes and their labels.
pub(crate) const EOL_BOX: u32 = 0xdadada;
pub(crate) const EOL_TEXT: u32 = 0x57606a;
/// Indentation guides ("Indent guideline style").
pub(crate) const INDENT_GUIDE: u32 = 0xc0c0c0;
/// The vertical edge, as a line or a background ("Edge colour").
pub(crate) const EDGE: u32 = 0x80ffff;
/// The current line's background or frame ("Current line background colour").
pub(crate) const CURRENT_LINE: u32 = 0xe8e8ff;

/// How highlighted text looks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TextStyle {
    pub(crate) color: u32,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
}

const fn style(color: u32) -> TextStyle {
    TextStyle {
        color,
        bold: false,
        italic: false,
    }
}

const fn bold(color: u32) -> TextStyle {
    TextStyle {
        bold: true,
        ..style(color)
    }
}

const fn italic(color: u32) -> TextStyle {
    TextStyle {
        italic: true,
        ..style(color)
    }
}

/// Syntax colors in the spirit of Notepad++'s default style: blue bold keywords, purple types,
/// green comments, grey strings, orange numbers, navy bold operators, brown preprocessor.
/// Names not listed fall back to their prefix (`function.method` to `function`), then to the
/// plain text color.
const SYNTAX: &[(&str, TextStyle)] = &[
    ("attribute", style(0x804000)),
    ("boolean", bold(0x0000ff)),
    ("character", style(0x808080)),
    ("comment", style(0x008000)),
    ("constant", style(0x8000ff)),
    ("constant.builtin", bold(0x0000ff)),
    ("constructor", style(0x8000ff)),
    ("diff.delta", style(0x8000ff)),
    ("diff.minus", style(0xd1242f)),
    ("diff.plus", style(0x1a7f37)),
    ("error", style(0xff0000)),
    ("escape", style(0xff8000)),
    ("function", style(0x000080)),
    ("function.macro", style(0x804000)),
    ("keyword", bold(0x0000ff)),
    ("keyword.directive", style(0x804000)),
    ("label", style(0x008080)),
    ("markup.heading", bold(0x0000ff)),
    ("markup.italic", italic(0x1f2328)),
    ("markup.link", style(0x0000ff)),
    ("markup.list", style(0xff8000)),
    ("markup.quote", italic(0x808080)),
    ("markup.raw", style(0x808080)),
    ("markup.strong", bold(0x1f2328)),
    ("module", style(0x008080)),
    ("number", style(0xff8000)),
    ("operator", bold(0x000080)),
    ("string", style(0x808080)),
    ("string.escape", style(0xff8000)),
    ("string.regexp", style(0xc00000)),
    ("string.special.key", style(0x000080)),
    ("tag", style(0x0000ff)),
    ("tag.attribute", style(0xff0000)),
    ("type", style(0x8000ff)),
    ("variable.builtin", style(0x0000ff)),
];

/// The style of a syntax highlight, if it has one.
pub(crate) fn syntax_style(highlight: birchpad_syntax::Highlight) -> Option<TextStyle> {
    static TABLE: std::sync::OnceLock<Vec<Option<TextStyle>>> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        birchpad_syntax::HIGHLIGHT_NAMES
            .iter()
            .map(|name| {
                let mut name: &str = name;
                loop {
                    if let Some((_, found)) = SYNTAX.iter().find(|(known, _)| *known == name) {
                        return Some(*found);
                    }
                    name = &name[..name.rfind('.')?];
                }
            })
            .collect()
    });
    table[usize::from(highlight.0)]
}

/// A bracket at the caret and its partner, as Notepad++ shows them.
pub(crate) const BRACE_MATCH: TextStyle = bold(0xff0000);
/// A bracket without a partner.
pub(crate) const BRACE_BAD: TextStyle = bold(0x800000);
