//! Installing updates of a per-user installation (ADR 0020): Velopack's, on Windows, with the
//! `installer` feature. Other copies (portable, the MSI, macOS, Linux, development builds) have
//! no installer; Birchpad announces their updates with the download page.

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

/// The installer of this copy, if it was installed per user and can update itself.
pub fn detect() -> Option<Arc<dyn Installer>> {
    #[cfg(all(windows, feature = "installer"))]
    {
        velopack_installer::detect()
    }
    #[cfg(not(all(windows, feature = "installer")))]
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
    #[cfg(not(all(windows, feature = "installer")))]
    let _ = (installed, updated, uninstalling);
}

#[cfg(all(windows, feature = "installer"))]
mod velopack_installer {
    //! Per-user installations made by Velopack's installer update themselves with it. The
    //! packages come from the signed manifest: Velopack is given a feed of exactly the package
    //! the manifest lists, which is downloaded and checked against its size and SHA-256 before
    //! Velopack checks it again and installs it.

    use std::path::Path;
    use std::sync::mpsc::Sender;
    use std::sync::{Arc, Mutex};

    use semver::Version;
    use velopack::sources::UpdateSource;
    use velopack::{
        UpdateCheck, UpdateManager, UpdateOptions, VelopackAsset, VelopackAssetFeed, bundle,
    };

    use super::{Installer, Progress};
    use crate::manifest::PACKAGE_ID;
    use crate::{Package, Transport};

    /// The installer, if Birchpad runs from a Velopack installation.
    pub(super) fn detect() -> Option<Arc<dyn Installer>> {
        UpdateManager::new(velopack::sources::NoneSource {}, None, None).ok()?;
        Some(Arc::new(Velopack::default()))
    }

    #[derive(Default)]
    struct Velopack {
        /// The manager and the package of a downloaded update.
        downloaded: Mutex<Option<(UpdateManager, VelopackAsset)>>,
    }

    impl Installer for Velopack {
        fn download(
            &self,
            version: &Version,
            package: &Package,
            transport: Arc<dyn Transport>,
            progress: Progress,
        ) -> Result<(), String> {
            let source = SignedSource {
                version: version.to_string(),
                package: package.clone(),
                transport,
                progress,
            };
            let options = UpdateOptions {
                AllowVersionDowngrade: false,
                ExplicitChannel: None,
                // Only full packages are listed.
                MaximumDeltasBeforeFallback: -1,
            };
            let manager = UpdateManager::new(source, Some(options), None)
                .map_err(|error| error.to_string())?;
            let UpdateCheck::UpdateAvailable(update) = manager
                .check_for_updates()
                .map_err(|error| error.to_string())?
            else {
                return Err(format!("{version} is not newer than the installed version"));
            };
            manager
                .download_updates(&update, None)
                .map_err(|error| error.to_string())?;
            *self.downloaded.lock().expect("not poisoned") =
                Some((manager, update.TargetFullRelease.clone()));
            Ok(())
        }

        fn restart(&self) -> Result<(), String> {
            let downloaded = self.downloaded.lock().expect("not poisoned");
            let (manager, package) = downloaded.as_ref().ok_or("nothing was downloaded")?;
            manager
                .wait_exit_then_apply_updates(package, true, true, Vec::<String>::new())
                .map_err(|error| error.to_string())
        }
    }

    /// A Velopack feed of the one package of a release that the signed manifest lists.
    struct SignedSource {
        version: String,
        package: Package,
        transport: Arc<dyn Transport>,
        /// Told the bytes received, besides Velopack's percentage.
        progress: Progress,
    }

    impl UpdateSource for SignedSource {
        fn get_release_feed(
            &self,
            _channel: &str,
            _app: &bundle::Manifest,
            _staged_user_id: &str,
        ) -> Result<VelopackAssetFeed, velopack::Error> {
            Ok(VelopackAssetFeed {
                Assets: vec![VelopackAsset {
                    PackageId: PACKAGE_ID.to_owned(),
                    Version: self.version.clone(),
                    Type: "Full".to_owned(),
                    FileName: self.package.file.clone(),
                    SHA1: self.package.sha1.clone(),
                    SHA256: self.package.sha256.clone(),
                    Size: self.package.size,
                    NotesMarkdown: String::new(),
                    NotesHtml: String::new(),
                }],
            })
        }

        fn download_release_entry(
            &self,
            asset: &VelopackAsset,
            local_file: &Path,
            progress: Option<Sender<i16>>,
        ) -> Result<(), velopack::Error> {
            if asset.FileName != self.package.file {
                return Err(velopack::Error::Other(format!(
                    "{} is not the package of the manifest",
                    asset.FileName
                )));
            }
            let size = self.package.size.max(1);
            let report = |received: u64| {
                (self.progress)(received);
                if let Some(progress) = &progress {
                    progress.send((received * 100 / size) as i16).ok();
                }
            };
            self.transport
                .download(&self.package.url, local_file, self.package.size, &report)
                .map_err(velopack::Error::Other)?;
            crate::verify_package(local_file, &self.package).map_err(|error| {
                std::fs::remove_file(local_file).ok();
                velopack::Error::Other(error)
            })
        }
    }

    #[cfg(test)]
    #[path = "../../../tests/unit/install/velopack_installer/tests.rs"]
    mod tests;
}
