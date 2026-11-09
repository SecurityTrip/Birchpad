//! The crate's tests through its public interface, one module per part of `src`, in one
//! test binary.

#[path = "blame/tests.rs"]
mod blame;

#[path = "changes/tests.rs"]
mod changes;

#[path = "tests.rs"]
mod lib_tests;
