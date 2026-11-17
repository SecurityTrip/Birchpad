//! Installing updates with Velopack on Windows and macOS (ADR 0020). Birchpad announces updates
//! with the download page for copies without Velopack, including the macOS disk image.

use std::sync::Arc;

use semver::Version;

use crate::{Package, Transport};

/// Told the bytes of a package received so far, as they arrive.
pub type Progress = Arc<dyn Fn(u64) + Send + Sync>;

/// Installs updates of a per-user installation; tests use fakes.
pub trait Installer: Send + Sync {
    /// Downloads the package of `version` and checks it against the manifest (blocking), so that
    /// it is installed when Birchpad starts next, or by [`Installer::restart`].
    fn download(
        &self,
        version: &Version,
        package: &Package,
        transport: Arc<dyn Transport>,
        progress: Progress,
    ) -> Result<(), String>;

    /// Starts the installer of the downloaded update: it waits for Birchpad to quit, installs
    /// the update and starts Birchpad again.
    fn restart(&self) -> Result<(), String>;
}

/// The installer of this copy, if it can update itself.
pub fn detect() -> Option<Arc<dyn Installer>> {
    #[cfg(all(any(windows, target_os = "macos"), feature = "installer"))]
    {
        velopack_installer::detect()
    }
    #[cfg(not(all(any(windows, target_os = "macos"), feature = "installer")))]
    {
        None
    }
}

/// The installer's work at startup, before anything else: finishing an install or uninstall, or
/// installing an update downloaded during the last run (Birchpad then starts again).
/// `installed` runs once Birchpad is installed for the user, `updated` once an update is
/// installed, `uninstalling` before it is uninstalled (the Explorer's "Edit with Birchpad").
pub fn run_hooks(installed: fn(), updated: fn(), uninstalling: fn()) {
    #[cfg(all(windows, feature = "installer"))]
    velopack::VelopackApp::build()
        .on_after_install_fast_callback(move |_| installed())
        .on_after_update_fast_callback(move |_| updated())
        .on_before_uninstall_fast_callback(move |_| uninstalling())
        .run();
    #[cfg(all(target_os = "macos", feature = "installer"))]
    velopack::VelopackApp::build().run();
    #[cfg(not(all(windows, feature = "installer")))]
    let _ = (installed, updated, uninstalling);
}

#[cfg(all(any(windows, target_os = "macos"), feature = "installer"))]
pub mod velopack_installer;
