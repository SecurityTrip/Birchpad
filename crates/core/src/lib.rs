//! The text model of the Birchpad editor.
//!
//! This crate knows nothing about windows, fonts or keyboards, so everything in it can be tested
//! in isolation. All positions are **byte offsets** into the text; conversion to characters,
//! UTF-16 code units or visual columns happens at the edges of the application.

mod change;
mod decoration;
mod document;
mod format;
mod history;
mod line_ending;
pub mod motion;
pub mod ops;
pub mod search;
mod selection;
mod transaction;

pub use change::{Assoc, ChangeSet, Edit, InvalidEdit, Operation, PosMapper};
pub use decoration::{LineMarkers, RangeSet, map_ranges, next_in_any, previous_in_any};
pub use document::Document;
pub use format::{Encoding, Format};
pub use history::{History, RevisionId, UndoGrouping};
pub use line_ending::{LineEnding, convert_line_breaks};
pub use motion::LINE_TYPE;
pub use ropey::{LineType, Rope, RopeSlice};
pub use selection::{Range, Selection};
pub use transaction::Transaction;
