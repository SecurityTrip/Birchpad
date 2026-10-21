//! The crate's tests through its public interface, one module per part of `src`, in one
//! test binary.

#[path = "color/tests.rs"]
mod color;

#[path = "notepad/tests.rs"]
mod notepad;

#[path = "style/tests.rs"]
mod style;

#[path = "tests.rs"]
mod lib_tests;
