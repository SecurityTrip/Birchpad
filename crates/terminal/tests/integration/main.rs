//! The crate's tests through its public interface, one module per part of `src`, in one
//! test binary.

#[path = "emulator/tests.rs"]
mod emulator;

#[path = "keys/tests.rs"]
mod keys;

#[path = "screen/tests.rs"]
mod screen;

#[path = "session/tests.rs"]
mod session;
