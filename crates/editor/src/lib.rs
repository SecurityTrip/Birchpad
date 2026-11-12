//! Birchpad's editor: the editor view, one view of a document ([`buffer::Buffer`], of
//! `birchpad-buffer`), with its own selection, scroll position and layout, drawn in the active
//! theme ([`theme`], of `birchpad-widgets`).
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
mod hooks;

/// The document a view shows: `birchpad-buffer`'s, under the name it had here.
pub use birchpad_buffer as buffer;
/// The active theme and what parts of the window draw alike: `birchpad-widgets`'.
pub use birchpad_widgets::{self as widgets, MONOSPACE, theme};

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

pub use hooks::{
    ActiveFindOptions, DiagnosticLevel, Diagnostics, EditorHooks, GitMark, GitMarks, Ranges,
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
