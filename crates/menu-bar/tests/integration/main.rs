//! The crate's tests through its public interface, one module per part of `src`, in one
//! test binary.

// macOS has the native menu bar.
#[cfg(not(target_os = "macos"))]
#[path = "gpui_tests.rs"]
mod gpui_tests;

#[path = "tests.rs"]
mod lib_tests;
