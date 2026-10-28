//! Birchpad's editor: a document ([`buffer::Buffer`]) and the editor view, one view of a buffer,
//! with its own selection, scroll position and layout; the active theme ([`theme`]).
//!
//! Several views may show the same buffer (split view, "Clone to Other View"); each maps its
//! selection through the changes the others make. Layout in cells (tab stops, word wrap, rows)
//! comes from `birchpad-view`; [`element::EditorElement`] turns it into pixels and paints it.
//!
//! The editor registers its commands in `birchpad_ui`'s registry ([`register_commands`]) and
//! asks the application for the rest through [`EditorHooks`] and [`ActiveFindOptions`].

#[macro_use]
extern crate birchpad_i18n;

mod view;
pub use view::{
    EditorCommands, EditorEvent, EditorView, LastEdit, VIEW_SWITCHES, ViewSettings, ViewState,
    ViewSwitch, all_characters, register_commands, without_control_characters,
};

pub mod banner;
pub mod buffer;
mod hooks;

pub mod column_editor;
pub mod completion;
pub mod element;
pub mod folding;
pub mod layout;
pub mod marks;
pub mod multi;
pub mod operations;
pub mod smart_highlight;
pub mod symbols;
pub mod syntax_selection;
pub mod theme;

pub use hooks::{ActiveFindOptions, EditorHooks, Ranges};

/// The platform's monospaced font: the editor's when the theme names none.
pub const MONOSPACE: &str = if cfg!(windows) {
    "Consolas"
} else if cfg!(target_os = "macos") {
    "Menlo"
} else {
    "DejaVu Sans Mono"
};

/// How many colors a tab can be given (Notepad++'s five).
pub const TAB_COLORS: u8 = 5;

/// What a tab carries besides its document: pinned, and its color.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TabMarks {
    /// Pinned tabs stay at the start of the bar and Close All but Pinned leaves them.
    pub pinned: bool,
    /// 1 to [`TAB_COLORS`]; none if unset.
    pub color: Option<u8>,
}

/// Zoom levels are font size steps of one pixel, like Notepad++'s points.
pub const ZOOM_RANGE: std::ops::RangeInclusive<i32> = -8..=40;
