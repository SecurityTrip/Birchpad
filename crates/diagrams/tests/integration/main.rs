//! The crate's tests through its public interface, one module per part of `src`, in one
//! test binary.

#[path = "includes_tests.rs"]
mod includes;

#[path = "server_tests.rs"]
mod server;

#[path = "tests.rs"]
mod lib_tests;
