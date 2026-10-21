//! The crate's tests through its public interface, one module per part of `src`, in one
//! test binary.

#[path = "manager/tests.rs"]
mod manager;

#[path = "manifest/tests.rs"]
mod manifest;

#[path = "script/tests.rs"]
mod script;

#[path = "session/tests.rs"]
mod session;

#[path = "wasm/tests.rs"]
mod wasm;
