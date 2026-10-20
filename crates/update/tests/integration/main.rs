//! The crate's tests through its public interface, one module per part of `src`, in one
//! test binary.

#[cfg(all(windows, feature = "installer"))]
#[path = "install/velopack_installer/tests.rs"]
mod install_velopack_installer;

#[path = "manifest/tests.rs"]
mod manifest;

#[path = "tests.rs"]
mod lib_tests;
