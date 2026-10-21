//! The editor's tests that need the whole application: files on disk, both views, panels, the
//! command line.

#[path = "editor_tests/change_history_tests.rs"]
mod change_history_tests;
#[path = "editor_tests/command_sweep_tests.rs"]
mod command_sweep_tests;
#[path = "editor_tests/hidden_lines_tests.rs"]
mod hidden_lines_tests;
#[path = "editor_tests/links_tests.rs"]
mod links_tests;
