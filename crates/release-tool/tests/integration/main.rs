//! The crate's tests through its public interface, one module per part of `src`, in one
//! test binary.

#[path = "crate_versions/tests.rs"]
mod crate_versions;

#[path = "tests.rs"]
mod lib_tests;
