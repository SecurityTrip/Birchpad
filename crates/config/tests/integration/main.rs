//! The crate's tests through its public interface, one module per part of `src`, in one
//! test binary.

#[path = "api/tests.rs"]
mod api;

#[path = "layers/tests.rs"]
mod layers;

#[path = "load/tests.rs"]
mod load;

#[path = "native_lang/tests.rs"]
mod native_lang;

#[path = "notepad_session/tests.rs"]
mod notepad_session;

#[cfg(windows)]
#[path = "policy/registry/tests.rs"]
mod policy_registry;

#[path = "project/files_tests.rs"]
mod project_files_tests;

#[path = "project/tests.rs"]
mod project;

#[path = "session/tests.rs"]
mod session;

#[path = "session_names/tests.rs"]
mod session_names;

#[path = "settings/completion_tests.rs"]
mod settings_completion_tests;

#[path = "settings/link_tests.rs"]
mod settings_link_tests;

#[path = "settings/print_tests.rs"]
mod settings_print_tests;

#[path = "settings/search_engine_tests.rs"]
mod settings_search_engine_tests;

#[path = "state/tests.rs"]
mod state;

#[path = "write/tests.rs"]
mod write;
