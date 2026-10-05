//! Updates (ADR 0020): checking the signed update manifest, on request and in the background, and
//! installing updates of a per-user installation.
//!
//! - `updates.mode = "off"`: no checks, no network.
//! - `"notify"`: a check a day in the background; a newer release is announced once, with a way
//!   to install it (an installed copy) or to open its download page (a portable copy).
//! - `"auto"`: the same, except that an installed copy downloads the update itself, checks it
//!   against the manifest, and installs it when Birchpad restarts.
//!
//! Help > Check for Updates checks at once, whatever the mode but `off`. Development builds check
//! only then.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result};
use birchpad_config::UpdateMode;
use birchpad_update::manifest::{self, PLATFORM};
use birchpad_update::{
    CheckError, CheckRequest, Checked, HttpTransport, Outcome, Package, Release, Transport,
    VerifyingKey,
};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::notification::Notification;
use gpui_kit::{AnyWindowHandle, App, AppContext as _, Context, Global, WeakEntity, Window};
use semver::Version;

use crate::app_state::AppState;
use crate::commands::CommandRegistry;
use crate::help::{BUILD_CHANNEL, VERSION, show_message};
use crate::workspace::{Workspace, report_error};

/// The first background check waits this long after the start.
const FIRST_CHECK: Duration = Duration::from_secs(30);
/// How often whether a background check is due is looked at.
const POLL: Duration = Duration::from_secs(60 * 60);
/// Background checks are this far apart.
const INTERVAL: u64 = 20 * 60 * 60;

/// The build's time, below which update manifests are refused (`build.rs`).
const BUILD_TIME: &str = env!("BIRCHPAD_BUILD_TIME");

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("help.check-updates", |this, (), window, cx| {
        this.check_for_updates(Trigger::Request, window, cx)
    });
    registry.enabled_when("help.check-updates", |cx| {
        AppState::global(cx).settings.updates.mode != UpdateMode::Off
    });
    registry.workspace("help.update-now", |this, (), window, cx| {
        this.update_now(window, cx)
    });
    registry.enabled_when("help.update-now", |cx| {
        let updates = cx.global::<Updates>();
        updates.installer.is_some() && updates.available.is_some()
    });
    registry.workspace("help.restart-to-update", |this, (), window, cx| {
        this.restart_to_update(window, cx);
        Ok(())
    });
    registry.enabled_when("help.restart-to-update", |cx| {
        cx.global::<Updates>().ready.is_some()
    });
}

/// Installs updates of a per-user installation (Velopack on Windows); tests use fakes.
pub(crate) trait Installer: Send + Sync {
    /// Downloads the package of `version` and checks it against the manifest (blocking), so that
    /// it is installed when Birchpad starts next, or by [`Installer::restart`].
    fn download(
        &self,
        version: &Version,
        package: &Package,
        transport: Arc<dyn Transport>,
    ) -> Result<(), String>;

    /// Starts the installer of the downloaded update: it waits for Birchpad to quit, installs
    /// the update and starts Birchpad again.
    fn restart(&self) -> Result<(), String>;
}

/// How updates are checked and installed.
pub(crate) struct Updates {
    transport: Arc<dyn Transport>,
    keys: Vec<VerifyingKey>,
    /// Set when Birchpad was installed per user and can update itself.
    installer: Option<Arc<dyn Installer>>,
    /// The platform whose packages it installs ([`PLATFORM`]).
    platform: Option<&'static str>,
    checking: bool,
    downloading: bool,
    /// The newest release found, if it is newer than this one.
    available: Option<Release>,
    /// A downloaded update, installed at the next start.
    ready: Option<Version>,
}

impl Global for Updates {}

impl Updates {
    pub(crate) fn install(
        transport: Arc<dyn Transport>,
        keys: Vec<VerifyingKey>,
        installer: Option<Arc<dyn Installer>>,
        cx: &mut App,
    ) {
        cx.set_global(Self {
            transport,
            keys,
            installer,
            platform: PLATFORM,
            checking: false,
            downloading: false,
            available: None,
            ready: None,
        });
    }

    /// The real transport. Creating it does not touch the network.
    pub(crate) fn http() -> Arc<dyn Transport> {
        let user_agent = format!("Birchpad/{VERSION} ({})", std::env::consts::OS);
        Arc::new(HttpTransport::new(&user_agent))
    }

    pub(crate) fn installed(cx: &App) -> bool {
        cx.try_global::<Self>()
            .is_some_and(|updates| updates.installer.is_some())
    }
}

/// Who asked for a check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Trigger {
    /// Help > Check for Updates: every outcome is shown.
    Request,
    /// The daily check: only news is shown, once.
    Background,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// Whether this build checks in the background: release builds, and development builds with
/// `BIRCHPAD_BACKGROUND_UPDATES` set (for trying it out).
pub(crate) fn checks_in_background() -> bool {
    BUILD_CHANNEL != "development" || std::env::var_os("BIRCHPAD_BACKGROUND_UPDATES").is_some()
}

/// Checks for updates in the background while the window is open, when the mode is not `off`:
/// first shortly after the start, then once a day.
pub(crate) fn start_background_checks(
    window: AnyWindowHandle,
    workspace: WeakEntity<Workspace>,
    cx: &mut App,
) {
    if !checks_in_background() {
        return;
    }
    cx.spawn(async move |cx| {
        let mut delay = FIRST_CHECK;
        loop {
            cx.background_executor().timer(delay).await;
            delay = POLL;
            let due = cx.update(|cx| {
                let state = AppState::global(cx);
                let updates = cx.global::<Updates>();
                state.settings.updates.mode != UpdateMode::Off
                    && now().saturating_sub(state.state.updates.last_check) >= INTERVAL
                    && !updates.checking
                    && !updates.downloading
                    && updates.ready.is_none()
            });
            if !due {
                continue;
            }
            let checked = cx.update_window(window, |_, window, cx| {
                workspace
                    .update(cx, |workspace, cx| {
                        workspace
                            .check_for_updates(Trigger::Background, window, cx)
                            .ok();
                    })
                    .ok();
            });
            if checked.is_err() {
                break;
            }
        }
    })
    .detach();
}

impl Workspace {
    /// Reads the update manifest and deals with what it says.
    pub(crate) fn check_for_updates(
        &mut self,
        trigger: Trigger,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let settings = AppState::global(cx).settings.updates.clone();
        if settings.mode == UpdateMode::Off {
            return Err(CheckError::Disabled.into());
        }
        let updates = cx
            .try_global::<Updates>()
            .context("checking for updates is not available")?;
        if updates.checking {
            return Ok(());
        }
        if trigger == Trigger::Request
            && let Some(version) = updates.ready.clone()
        {
            show_ready(&version, cx.entity().downgrade(), window, cx);
            return Ok(());
        }
        let transport = updates.transport.clone();
        let keys = updates.keys.clone();
        let platform = updates.platform;
        let newest_seen = AppState::global(cx)
            .state
            .updates
            .manifest_timestamp
            .max(BUILD_TIME.parse().unwrap_or(0));
        cx.global_mut::<Updates>().checking = true;
        let current = Version::parse(VERSION).expect("the version is semver");
        let check = cx.background_spawn(async move {
            let request = CheckRequest {
                settings: &settings,
                current: &current,
                platform,
                keys: &keys,
                now: now(),
                newest_seen,
            };
            birchpad_update::check(&request, transport.as_ref())
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = check.await;
            cx.update(|window, cx| {
                // Even if the workspace went away meanwhile, so that the next check runs.
                cx.global_mut::<Updates>().checking = false;
                this.update(cx, |this, cx| this.checked(result, trigger, window, cx))
                    .ok();
            })
            .ok();
        })
        .detach();
        Ok(())
    }

    fn checked(
        &mut self,
        result: Result<Checked, CheckError>,
        trigger: Trigger,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let checked = match result {
            Ok(checked) => checked,
            Err(error) => {
                if trigger == Trigger::Request {
                    show_message(
                        "Cannot Check for Updates",
                        vec![error.to_string()],
                        None,
                        window,
                        cx,
                    );
                } else {
                    eprintln!("update check: {error}");
                }
                return;
            }
        };
        AppState::update_state(cx, |state, _| {
            state.updates.last_check = now();
            state.updates.manifest_timestamp =
                state.updates.manifest_timestamp.max(checked.timestamp);
        });
        let release = match checked.outcome {
            Outcome::Available(release) => release,
            Outcome::UpToDate { newest } => {
                cx.global_mut::<Updates>().available = None;
                if trigger == Trigger::Request {
                    up_to_date(newest, window, cx);
                }
                return;
            }
        };
        cx.global_mut::<Updates>().available = Some(release.clone());
        let updates = cx.global::<Updates>();
        let installable = updates.installer.is_some() && release.package.is_some();
        let auto = AppState::global(cx).settings.updates.mode == UpdateMode::Auto;
        let workspace = cx.entity().downgrade();
        match trigger {
            Trigger::Request => show_available(&release, installable, workspace, window, cx),
            Trigger::Background if installable && auto => {
                self.download_update(release, AfterDownload::Announce, window, cx);
            }
            Trigger::Background => {
                let version = release.version.to_string();
                if AppState::global(cx).state.updates.announced.as_deref() != Some(&version) {
                    AppState::update_state(cx, |state, _| state.updates.announced = Some(version));
                    announce_available(&release, installable, workspace, window, cx);
                }
            }
        }
    }

    /// Help > Check for Updates, Update and Restart; a notification's Update.
    fn update_now(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Result<()> {
        let updates = cx.global::<Updates>();
        if let Some(version) = updates.ready.clone() {
            show_ready(&version, cx.entity().downgrade(), window, cx);
            return Ok(());
        }
        let release = updates
            .available
            .clone()
            .context("no newer version was found")?;
        self.download_update(release, AfterDownload::Restart, window, cx);
        Ok(())
    }

    fn download_update(
        &mut self,
        release: Release,
        after: AfterDownload,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let updates = cx.global::<Updates>();
        let (Some(installer), Some(package)) = (updates.installer.clone(), release.package.clone())
        else {
            return;
        };
        if updates.downloading {
            return;
        }
        let transport = updates.transport.clone();
        cx.global_mut::<Updates>().downloading = true;
        if after == AfterDownload::Restart {
            window.push_notification(
                Notification::info(format!("Downloading Birchpad {}…", release.version)),
                cx,
            );
        }
        let version = release.version.clone();
        let download =
            cx.background_spawn(async move { installer.download(&version, &package, transport) });
        cx.spawn_in(window, async move |this, cx| {
            let result = download.await;
            cx.update(|window, cx| {
                cx.global_mut::<Updates>().downloading = false;
                match result {
                    Ok(()) => {
                        cx.global_mut::<Updates>().ready = Some(release.version.clone());
                        match after {
                            AfterDownload::Restart => {
                                this.update(cx, |this, cx| this.restart_to_update(window, cx))
                                    .ok();
                            }
                            AfterDownload::Announce => {
                                announce_ready(&release.version, this.clone(), window, cx);
                            }
                        }
                    }
                    Err(error) => {
                        let message =
                            format!("Cannot download Birchpad {}: {error}", release.version);
                        if after == AfterDownload::Restart {
                            report_error(&anyhow::anyhow!(message), window, cx);
                        } else {
                            eprintln!("{message}");
                            announce_available(&release, false, this.clone(), window, cx);
                        }
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    /// Quits into the installer of the downloaded update, which starts Birchpad again: first
    /// as when quitting, saving the session (or asking about unsaved changes).
    pub(crate) fn restart_to_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(installer) = cx.global::<Updates>().installer.clone() else {
            return;
        };
        if cx.global::<Updates>().ready.is_none() {
            return;
        }
        let ready = self.prepare_to_quit(window, cx);
        cx.spawn_in(window, async move |_, cx| {
            if !ready.await {
                return;
            }
            cx.update(|window, cx| match installer.restart() {
                Ok(()) => cx.quit(),
                Err(error) => report_error(
                    &anyhow::anyhow!("Cannot start the update: {error}"),
                    window,
                    cx,
                ),
            })
            .ok();
        })
        .detach();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AfterDownload {
    /// Tell the user it will be installed at the next start.
    Announce,
    /// Restart at once (the user asked for it).
    Restart,
}

fn up_to_date(newest: Option<Version>, window: &mut Window, cx: &mut App) {
    let channel = crate::help::channel_name(AppState::global(cx).settings.updates.channel);
    let line = match newest {
        Some(newest) => {
            format!("You have version {VERSION}; the newest {channel} release is {newest}.")
        }
        None => format!("You have version {VERSION}; no {channel} release is published yet."),
    };
    show_message("Birchpad Is Up to Date", vec![line], None, window, cx);
}

/// The answer to Help > Check for Updates when there is a newer release: Update and Restart for
/// an installed copy, Open Download Page otherwise.
fn show_available(
    release: &Release,
    installable: bool,
    workspace: WeakEntity<Workspace>,
    window: &mut Window,
    cx: &mut App,
) {
    let mut lines = vec![format!(
        "Birchpad {} is available. You have version {VERSION}.",
        release.version
    )];
    let action = if installable {
        lines.push(
            "It is downloaded and checked against the signed update manifest; Birchpad then \
             restarts, reopening your documents."
                .to_owned(),
        );
        crate::help::MessageAction::Run(
            "Update and Restart",
            Box::new(move |window, cx| {
                workspace
                    .update(cx, |workspace, cx| {
                        if let Err(error) = workspace.update_now(window, cx) {
                            report_error(&error, window, cx);
                        }
                    })
                    .ok();
            }),
        )
    } else {
        lines.push(format!("Download page: {}", release.page));
        crate::help::MessageAction::Open("Open Download Page", release.page.clone())
    };
    crate::help::show_message_with("Update Available", lines, Some(action), window, cx);
}

fn show_ready(
    version: &Version,
    workspace: WeakEntity<Workspace>,
    window: &mut Window,
    cx: &mut App,
) {
    crate::help::show_message_with(
        "Update Ready",
        vec![format!(
            "Birchpad {version} has been downloaded. It is installed when Birchpad restarts."
        )],
        Some(crate::help::MessageAction::Run(
            "Restart Now",
            Box::new(move |window, cx| restart(&workspace, window, cx)),
        )),
        window,
        cx,
    );
}

fn restart(workspace: &WeakEntity<Workspace>, window: &mut Window, cx: &mut App) {
    workspace
        .update(cx, |workspace, cx| workspace.restart_to_update(window, cx))
        .ok();
}

/// A background check found a newer release: a notification with Update (an installed copy) or
/// Download (its page).
fn announce_available(
    release: &Release,
    installable: bool,
    workspace: WeakEntity<Workspace>,
    window: &mut Window,
    cx: &mut App,
) {
    let page = release.page.clone();
    let notification = Notification::new()
        .title(format!("Birchpad {} is available", release.version))
        .message(format!("You have version {VERSION}."))
        .action(move |_, _, cx| {
            let notification = cx.entity().downgrade();
            if installable {
                let workspace = workspace.clone();
                Button::new("update")
                    .primary()
                    .label("Update")
                    .on_click(move |_, window, cx| {
                        notification.update(cx, |n, cx| n.dismiss(window, cx)).ok();
                        workspace
                            .update(cx, |workspace, cx| {
                                if let Err(error) = workspace.update_now(window, cx) {
                                    report_error(&error, window, cx);
                                }
                            })
                            .ok();
                    })
            } else {
                let page = page.clone();
                Button::new("download")
                    .primary()
                    .label("Download")
                    .on_click(move |_, window, cx| {
                        notification.update(cx, |n, cx| n.dismiss(window, cx)).ok();
                        cx.open_url(&page);
                    })
            }
        });
    window.push_notification(notification, cx);
}

fn announce_ready(
    version: &Version,
    workspace: WeakEntity<Workspace>,
    window: &mut Window,
    cx: &mut App,
) {
    let notification = Notification::new()
        .title(format!("Birchpad {version} is ready"))
        .message("It is installed when Birchpad restarts.")
        .action(move |_, _, cx| {
            let notification = cx.entity().downgrade();
            let workspace = workspace.clone();
            Button::new("restart")
                .primary()
                .label("Restart Now")
                .on_click(move |_, window, cx| {
                    notification.update(cx, |n, cx| n.dismiss(window, cx)).ok();
                    restart(&workspace, window, cx);
                })
        });
    window.push_notification(notification, cx);
}

/// The trusted keys compiled into Birchpad.
pub(crate) fn trusted_keys() -> Vec<VerifyingKey> {
    manifest::trusted_keys()
}

/// How this copy of Birchpad is updated, for Help > About.
pub(crate) fn describe(cx: &App) -> String {
    let state = AppState::global(cx);
    let settings = &state.settings.updates;
    if settings.mode == UpdateMode::Off {
        return "turned off".to_owned();
    }
    let channel = crate::help::channel_name(settings.channel);
    let source = birchpad_update::manifest_url(settings);
    let how = if !checks_in_background() {
        "checked only from Help > Check for Updates (development build)"
    } else if settings.mode == UpdateMode::Auto && Updates::installed(cx) {
        "checked daily, installed when Birchpad restarts"
    } else if Updates::installed(cx) {
        "checked daily, new versions announced"
    } else {
        "checked daily, new versions announced with their download page"
    };
    let last = match state.state.updates.last_check {
        0 => String::new(),
        time => format!("; last checked {}", manifest::date(time)),
    };
    format!("{channel} channel from {source}, {how}{last}")
}

#[cfg(windows)]
pub(crate) mod velopack_installer {
    //! Per-user installations made by Velopack's installer update themselves with it. The
    //! packages come from the signed manifest: Velopack is given a feed of exactly the package
    //! the manifest lists, which is downloaded and checked against its size and SHA-256 before
    //! Velopack checks it again and installs it.

    use std::path::Path;
    use std::sync::mpsc::Sender;
    use std::sync::{Arc, Mutex};

    use birchpad_update::manifest::PACKAGE_ID;
    use birchpad_update::{Package, Transport};
    use semver::Version;
    use velopack::sources::UpdateSource;
    use velopack::{
        UpdateCheck, UpdateManager, UpdateOptions, VelopackAsset, VelopackAssetFeed, bundle,
    };

    use super::Installer;

    /// The installer, if Birchpad runs from a Velopack installation.
    pub(crate) fn detect() -> Option<Arc<dyn Installer>> {
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
        ) -> Result<(), String> {
            let source = SignedSource {
                version: version.to_string(),
                package: package.clone(),
                transport,
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
                if let Some(progress) = &progress {
                    progress.send((received * 100 / size) as i16).ok();
                }
            };
            self.transport
                .download(&self.package.url, local_file, self.package.size, &report)
                .map_err(velopack::Error::Other)?;
            birchpad_update::verify_package(local_file, &self.package).map_err(|error| {
                std::fs::remove_file(local_file).ok();
                velopack::Error::Other(error)
            })
        }
    }
}

/// The installer of this copy, if it was installed per user and can update itself.
pub(crate) fn detect_installer() -> Option<Arc<dyn Installer>> {
    #[cfg(windows)]
    {
        velopack_installer::detect()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Velopack's work at startup, before anything else: finishing an install or uninstall, or
/// installing an update downloaded during the last run (Birchpad then starts again).
pub(crate) fn run_installer_hooks() {
    #[cfg(windows)]
    velopack::VelopackApp::build().run();
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use birchpad_commands::Invocation;
    use birchpad_config::UpdateChannel;
    use birchpad_update::manifest::{PRODUCT, SCHEMA};
    use birchpad_update::{Manifest, ManifestRelease, SigningKey};
    use gpui_kit::{Entity, TestAppContext, VisualTestContext};

    use super::*;
    use crate::workspace::tests::open_workspace;

    fn key() -> SigningKey {
        SigningKey::from_bytes(&[9; 32])
    }

    /// Serves a signed manifest announcing `version`, signed at `timestamp`.
    struct Fake {
        manifest: Mutex<Vec<u8>>,
        requests: Mutex<usize>,
    }

    impl Fake {
        fn announce(&self, version: &str, timestamp: u64) {
            let manifest = Manifest {
                schema: SCHEMA,
                product: PRODUCT.into(),
                timestamp,
                expires: timestamp + 30 * 86_400,
                releases: vec![ManifestRelease {
                    version: Version::parse(version).unwrap(),
                    page: format!("https://example.com/{version}"),
                    packages: vec![Package {
                        platform: "windows-x64".into(),
                        file: format!("Birchpad-{version}-full.nupkg"),
                        url: format!("https://example.com/{version}.nupkg"),
                        size: 1,
                        sha256: String::new(),
                        sha1: String::new(),
                    }],
                }],
            };
            let envelope = manifest::sign(&manifest, &[key()]);
            *self.manifest.lock().unwrap() = serde_json::to_vec(&envelope).unwrap();
        }
    }

    impl Transport for Fake {
        fn get(&self, _: &str) -> Result<Vec<u8>, String> {
            *self.requests.lock().unwrap() += 1;
            Ok(self.manifest.lock().unwrap().clone())
        }
    }

    #[derive(Default)]
    struct FakeInstaller {
        calls: Mutex<Vec<String>>,
    }

    impl Installer for FakeInstaller {
        fn download(
            &self,
            version: &Version,
            _: &Package,
            _: Arc<dyn Transport>,
        ) -> Result<(), String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("download {version}"));
            Ok(())
        }

        fn restart(&self) -> Result<(), String> {
            self.calls.lock().unwrap().push("restart".into());
            Ok(())
        }
    }

    fn install(installed: bool, cx: &mut VisualTestContext) -> (Arc<Fake>, Arc<FakeInstaller>) {
        let fake = Arc::new(Fake {
            manifest: Mutex::default(),
            requests: Mutex::new(0),
        });
        fake.announce("99.0.0", now());
        let installer = Arc::new(FakeInstaller::default());
        let as_installer: Option<Arc<dyn Installer>> =
            installed.then(|| installer.clone() as Arc<dyn Installer>);
        cx.update(|_, cx| {
            Updates::install(fake.clone(), vec![key().verifying_key()], as_installer, cx);
            // The same on every platform the tests run on.
            cx.global_mut::<Updates>().platform = Some("windows-x64");
            let updates = &mut cx.global_mut::<AppState>().settings.updates;
            updates.mode = UpdateMode::Notify;
            updates.channel = UpdateChannel::Stable;
        });
        (fake, installer)
    }

    fn set_mode(mode: UpdateMode, cx: &mut VisualTestContext) {
        cx.update(|_, cx| cx.global_mut::<AppState>().settings.updates.mode = mode);
    }

    fn dialog_open(cx: &mut VisualTestContext) -> bool {
        cx.update(|window, cx| window.has_active_dialog(cx))
    }

    fn notifications(cx: &mut VisualTestContext) -> usize {
        cx.update(|window, cx| window.notifications(cx).len())
    }

    fn check(workspace: &Entity<Workspace>, trigger: Trigger, cx: &mut VisualTestContext) {
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.check_for_updates(trigger, window, cx).unwrap();
        });
        cx.run_until_parked();
    }

    #[gpui_kit::test]
    fn offers_the_download_page_of_a_newer_release(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        let (fake, _) = install(false, cx);
        check(&workspace, Trigger::Request, cx);
        assert!(dialog_open(cx));
        // Enter confirms: Open Download Page.
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert!(!dialog_open(cx));
        assert_eq!(
            cx.opened_url().as_deref(),
            Some("https://example.com/99.0.0")
        );
        assert_eq!(*fake.requests.lock().unwrap(), 1);
    }

    #[gpui_kit::test]
    fn off_disables_the_command_and_makes_no_request(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        let (fake, _) = install(false, cx);
        set_mode(UpdateMode::Off, cx);
        let enabled = cx.update(|_, cx| {
            cx.global::<CommandRegistry>()
                .is_enabled("help.check-updates", cx)
        });
        assert!(!enabled);
        workspace.update_in(cx, |workspace, window, cx| {
            let result = workspace.dispatch(&Invocation::new("help.check-updates"), window, cx);
            assert!(result.is_err());
            assert!(
                workspace
                    .check_for_updates(Trigger::Background, window, cx)
                    .is_err()
            );
        });
        cx.run_until_parked();
        assert!(!dialog_open(cx));
        assert_eq!(*fake.requests.lock().unwrap(), 0);
    }

    #[gpui_kit::test]
    fn an_installed_copy_updates_and_restarts(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        let (_, installer) = install(true, cx);
        check(&workspace, Trigger::Request, cx);
        assert!(dialog_open(cx));
        // Update and Restart.
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(
            *installer.calls.lock().unwrap(),
            ["download 99.0.0", "restart"]
        );
        let saved = cx.update(|_, cx| AppState::global(cx).state.updates.manifest_timestamp);
        assert!(saved > 0, "the manifest's time is remembered");
    }

    #[gpui_kit::test]
    fn background_checks_announce_a_version_once(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        let (fake, installer) = install(true, cx);
        check(&workspace, Trigger::Background, cx);
        assert!(!dialog_open(cx), "no dialog in the background");
        assert_eq!(notifications(cx), 1);
        check(&workspace, Trigger::Background, cx);
        assert_eq!(notifications(cx), 1, "announced once");
        assert!(installer.calls.lock().unwrap().is_empty(), "notify only");

        // A newer version is announced again.
        fake.announce("99.1.0", now());
        check(&workspace, Trigger::Background, cx);
        assert_eq!(notifications(cx), 2);
    }

    #[gpui_kit::test]
    fn auto_mode_downloads_and_waits_for_a_restart(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        let (_, installer) = install(true, cx);
        set_mode(UpdateMode::Auto, cx);
        check(&workspace, Trigger::Background, cx);
        assert_eq!(*installer.calls.lock().unwrap(), ["download 99.0.0"]);
        assert_eq!(notifications(cx), 1, "Restart Now");
        let ready = cx.update(|_, cx| cx.global::<Updates>().ready.clone());
        assert_eq!(ready, Some(Version::new(99, 0, 0)));

        // Asking now offers the restart instead of checking again.
        check(&workspace, Trigger::Request, cx);
        assert!(dialog_open(cx));
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(
            *installer.calls.lock().unwrap(),
            ["download 99.0.0", "restart"]
        );
    }

    #[gpui_kit::test]
    fn an_older_manifest_than_one_seen_is_refused(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        let (fake, _) = install(false, cx);
        let seen = now();
        cx.update(|_, cx| {
            cx.global_mut::<AppState>().state.updates.manifest_timestamp = seen;
        });
        fake.announce("99.0.0", seen - 3_600);
        check(&workspace, Trigger::Background, cx);
        assert_eq!(notifications(cx), 0);
        check(&workspace, Trigger::Request, cx);
        assert!(dialog_open(cx), "Cannot Check for Updates");
        assert_eq!(cx.opened_url(), None);
        let state = cx.update(|_, cx| AppState::global(cx).state.updates.clone());
        assert_eq!((state.manifest_timestamp, state.last_check), (seen, 0));
    }
}
