//! The crate's tests through its public interface, one module per part of `src`, in one
//! test binary: what the panels work out without a window. The panels in their docks are
//! tested with the application.

#[path = "character_panel_tests.rs"]
mod character_panel;

#[path = "clipboard_history_tests.rs"]
mod clipboard_history;

#[path = "document_map_tests.rs"]
mod document_map;

#[path = "function_list_tests.rs"]
mod function_list;
