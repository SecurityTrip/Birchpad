//! The crate's tests through its public interface, one module per part of `src`, in one
//! test binary.

#[path = "args/tests.rs"]
mod args;

/// Linux's `xdg-mime`, only what reads: registering would change the user's defaults.
#[cfg(all(unix, not(target_os = "macos")))]
#[path = "associations/linux.rs"]
mod associations_linux;

#[path = "associations/tests.rs"]
mod associations;

#[path = "explorer/tests.rs"]
mod explorer;

#[path = "instance/tests.rs"]
mod instance;
