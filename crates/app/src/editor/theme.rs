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
    #[expect(dead_code, reason = "used by brace matching (stage 2)")]
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

/// The bookmark symbol in the symbol margin.
pub(crate) const BOOKMARK: u32 = 0x2f6fde;
pub(crate) const GUTTER_BACKGROUND: u32 = 0xf6f8fa;
pub(crate) const GUTTER_TEXT: u32 = 0x8c959f;
/// A thin line between the margins and the text.
pub(crate) const GUTTER_BORDER: u32 = 0xe4e7eb;
