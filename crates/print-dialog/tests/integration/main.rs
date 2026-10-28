//! The crate's tests through its public interface, in one test binary; the crate is Windows'.

#[cfg(windows)]
#[path = "tests.rs"]
mod lib_tests;
