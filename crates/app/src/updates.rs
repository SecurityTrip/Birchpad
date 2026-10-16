//! Updates (ADR 0020): checking the signed update manifest, on request and in the background, and
//! installing updates of a per-user installation.
//!
//! - `updates.mode = "off"`: no checks, no network.
//! - `"notify"`: a check a day in the background; a newer release is announced once, with a way
//!   to install it (an installed copy) or to open its download page (a portable copy), and a
//!   "Don't show again" check box that turns the checks off.
//! - `"auto"`: the same, except that an installed copy downloads the update itself, checks it
//!   against the manifest, and installs it when Birchpad restarts.
//!
//! Help > Check for Updates checks at once, whatever the mode but `off`. Development builds check
//! only then.
//!
//! A download shows a bar where the buttons of the window it was asked for from were; one that
//! no window shows (the one `"auto"` starts, Help > Update and Restart) has a bar in the status
//! bar.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result};
use birchpad_config::UpdateMode;
use birchpad_update::install::Installer;
use birchpad_update::manifest::{self, PLATFORM};
use birchpad_update::{
    CheckError, CheckRequest, Checked, HttpTransport, Outcome, Release, Transport, VerifyingKey,
};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Sizable as _, WindowExt as _};
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Div, Global, WeakEntity, Window,
    div, prelude::*, px, relative,
};
use semver::Version;

use crate::app_state::AppState;
use crate::commands::{CommandRegistry, Register as _};
use crate::help::{BUILD_CHANNEL, VERSION, show_message};
use crate::theme::{paint, ui};
use crate::workspace::{Workspace, dialog_action, dialog_close, report_error};

/// The first background check waits this long after the start.
const FIRST_CHECK: Duration = Duration::from_secs(30);
/// How often whether a background check is due is looked at.
const POLL: Duration = Duration::from_secs(60 * 60);
/// Background checks are this far apart.
const INTERVAL: u64 = 20 * 60 * 60;
/// How often the bars of a download are redrawn.
const REDRAW: Duration = Duration::from_millis(250);
/// A download that received nothing for this long is shown as stalled; it fails after
/// [`birchpad_update::STALL`].
const SHOWN_STALLED: Duration = Duration::from_secs(10);

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
        this.update_now(None, window, cx)
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

/// How updates are checked and installed.
pub(crate) struct Updates {
    transport: Arc<dyn Transport>,
    keys: Vec<VerifyingKey>,
    /// Set when Birchpad was installed per user and can update itself.
    installer: Option<Arc<dyn Installer>>,
    /// The platform whose packages it installs ([`PLATFORM`]).
    platform: Option<&'static str>,
    checking: bool,
    /// The update being downloaded.
    download: Option<Download>,
    /// The last download that failed, and why.
    failed: Option<(Version, String)>,
    /// The newest release found, if it is newer than this one.
    available: Option<Release>,
    /// A downloaded update, installed at the next start.
    ready: Option<Version>,
}

/// An update being downloaded: what its bars show.
#[derive(Clone)]
pub(crate) struct Download {
    pub(crate) version: Version,
    /// The package's size, from the manifest.
    pub(crate) size: u64,
    received: Arc<Received>,
    /// The window the user updated from, while it is open; otherwise the status bar shows it.
    window: Option<UpdateWindow>,
}

/// A window that announces an update, which shows the bar of its download.
#[derive(Clone)]
enum UpdateWindow {
    /// Help > Check for Updates.
    Dialog,
    /// The announcement of a background check.
    Notification(WeakEntity<Notification>),
}

/// The bytes received so far and when the last of them arrived, written by the download.
struct Received {
    bytes: AtomicU64,
    at: Mutex<Instant>,
}

impl Download {
    fn new(version: Version, size: u64, window: Option<UpdateWindow>) -> Self {
        Self {
            version,
            size,
            received: Arc::new(Received {
                bytes: AtomicU64::new(0),
                at: Mutex::new(Instant::now()),
            }),
            window,
        }
    }

    /// Takes the bytes received so far.
    fn progress(&self) -> birchpad_update::install::Progress {
        let received = self.received.clone();
        Arc::new(move |bytes| {
            received.bytes.store(bytes, Ordering::Relaxed);
            *received.at.lock().expect("not poisoned") = Instant::now();
        })
    }

    pub(crate) fn received(&self) -> u64 {
        self.received.bytes.load(Ordering::Relaxed).min(self.size)
    }

    /// The share received, from 0 to 1.
    pub(crate) fn fraction(&self) -> f32 {
        if self.size == 0 {
            return 0.;
        }
        (self.received() as f64 / self.size as f64) as f32
    }

    /// How long nothing has arrived, if it is [`SHOWN_STALLED`] or longer at `now`.
    pub(crate) fn stalled(&self, now: Instant) -> Option<Duration> {
        let at = *self.received.at.lock().expect("not poisoned");
        Some(now.saturating_duration_since(at)).filter(|quiet| *quiet >= SHOWN_STALLED)
    }

    /// What is happening, under its bar and in the status bar's tooltip.
    pub(crate) fn describe(&self, now: Instant) -> String {
        match self.stalled(now) {
            Some(quiet) => t!(
                "updates.download-stalled",
                version = self.version,
                seconds = quiet.as_secs()
            ),
            None => t!(
                "updates.download-progress",
                version = self.version,
                received = megabytes(self.received()),
                size = megabytes(self.size)
            ),
        }
    }

    /// Whether it is shown in the status bar: no window shows it.
    pub(crate) fn in_status_bar(&self) -> bool {
        self.window.is_none()
    }
}

/// Bytes in megabytes, with a decimal.
fn megabytes(bytes: u64) -> String {
    t!(
        "updates.megabytes",
        size = format!("{:.1}", bytes as f64 / (1024. * 1024.))
    )
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
            download: None,
            failed: None,
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

    /// The update being downloaded, if any.
    pub(crate) fn download(cx: &App) -> Option<&Download> {
        cx.try_global::<Self>()?.download.as_ref()
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
                    && updates.download.is_none()
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
            .with_context(|| t!("updates.not-available"))?;
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
                        t!("updates.cannot-check"),
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
                self.download_update(release, AfterDownload::Announce, None, window, cx);
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

    /// Help > Check for Updates, Update and Restart; a notification's Update. `shown` is the
    /// window it was asked from, which shows the download's bar.
    fn update_now(
        &mut self,
        shown: Option<UpdateWindow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let updates = cx.global::<Updates>();
        if let Some(version) = updates.ready.clone() {
            show_ready(&version, cx.entity().downgrade(), window, cx);
            return Ok(());
        }
        let release = updates
            .available
            .clone()
            .with_context(|| t!("updates.no-newer-version"))?;
        self.download_update(release, AfterDownload::Restart, shown, window, cx);
        Ok(())
    }

    fn download_update(
        &mut self,
        release: Release,
        after: AfterDownload,
        shown: Option<UpdateWindow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let updates = cx.global::<Updates>();
        let (Some(installer), Some(package)) = (updates.installer.clone(), release.package.clone())
        else {
            return;
        };
        if updates.download.is_some() {
            return;
        }
        let transport = updates.transport.clone();
        let state = Download::new(release.version.clone(), package.size, shown);
        let progress = state.progress();
        let updates = cx.global_mut::<Updates>();
        updates.download = Some(state);
        updates.failed = None;
        cx.refresh_windows();
        redraw_while_downloading(cx);
        let version = release.version.clone();
        let download = cx.background_spawn(async move {
            installer.download(&version, &package, transport, progress)
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = download.await;
            cx.update(|window, cx| {
                let updates = cx.global_mut::<Updates>();
                let shown = updates.download.take().and_then(|download| download.window);
                if let Err(error) = &result {
                    updates.failed = Some((release.version.clone(), error.clone()));
                }
                cx.refresh_windows();
                match result {
                    Ok(()) => {
                        cx.global_mut::<Updates>().ready = Some(release.version.clone());
                        match after {
                            AfterDownload::Restart => {
                                close(shown, window, cx);
                                this.update(cx, |this, cx| this.restart_to_update(window, cx))
                                    .ok();
                            }
                            AfterDownload::Announce => {
                                announce_ready(&release.version, this.clone(), window, cx);
                            }
                        }
                    }
                    Err(error) => {
                        let message = t!(
                            "updates.cannot-download",
                            version = release.version,
                            error = error.to_string()
                        );
                        if after == AfterDownload::Restart {
                            // The window it was asked from says why, in place of the bar.
                            if shown.is_none() {
                                report_error(&anyhow::anyhow!(message), window, cx);
                            }
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
                    &anyhow::anyhow!("{}", t!("updates.cannot-start-the-update", error = error)),
                    window,
                    cx,
                ),
            })
            .ok();
        })
        .detach();
    }
}

/// Closes the window a finished download was asked from.
fn close(shown: Option<UpdateWindow>, window: &mut Window, cx: &mut App) {
    match shown {
        Some(UpdateWindow::Dialog) => window.close_dialog(cx),
        Some(UpdateWindow::Notification(notification)) => {
            notification
                .update(cx, |notification, cx| notification.dismiss(window, cx))
                .ok();
        }
        None => {}
    }
}

/// A window closed while its download goes on: the status bar shows it from now on.
fn forget_window(dialog: bool, cx: &mut App) {
    if !cx.has_global::<Updates>() {
        return;
    }
    if let Some(download) = cx.global_mut::<Updates>().download.as_mut()
        && matches!(download.window, Some(UpdateWindow::Dialog)) == dialog
    {
        download.window = None;
        cx.refresh_windows();
    }
}

/// The download of `version` in progress, if any.
fn downloading(version: &Version, cx: &App) -> Option<Download> {
    Updates::download(cx)
        .filter(|download| download.version == *version)
        .cloned()
}

/// Why the last download of `version` failed, if it did.
fn failure(version: &Version, cx: &App) -> Option<AnyElement> {
    let (_, error) = cx
        .global::<Updates>()
        .failed
        .as_ref()
        .filter(|(failed, _)| failed == version)?;
    Some(
        div()
            .debug_selector(|| "update-failed".into())
            .text_color(paint(ui().error))
            .child(t!(
                "updates.cannot-download",
                version = version,
                error = error.clone()
            ))
            .into_any_element(),
    )
}

/// A bar `fraction` full; `id` names its parts for tests (`{id}-track`, `{id}-fill`).
fn bar(fraction: f32, id: &'static str) -> Div {
    div()
        .debug_selector(move || format!("{id}-track"))
        .w_full()
        .h(px(6.))
        .rounded_full()
        .bg(paint(ui().border))
        .child(
            div()
                .debug_selector(move || format!("{id}-fill"))
                .h_full()
                .rounded_full()
                .w(relative(fraction.clamp(0., 1.)))
                .bg(paint(ui().accent)),
        )
}

/// What a window shows of its download in place of its buttons: the bar and what is happening.
fn download_bar(download: &Download, id: &'static str) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .gap_1()
        .child(bar(download.fraction(), id))
        .child(
            div()
                .text_color(paint(ui().muted))
                .child(download.describe(Instant::now())),
        )
}

#[cfg(test)]
thread_local! {
    /// The last tooltip of the status bar's download, for tests to read.
    static TOOLTIP: std::cell::RefCell<Option<String>> = std::cell::RefCell::default();
}

/// The status bar's bar of a download that no window shows; hovering it says what is
/// happening.
pub(crate) fn status_bar_item(download: Download) -> impl IntoElement {
    div()
        .id("status-update")
        .w(px(80.))
        .child(bar(download.fraction(), "status-update"))
        .tooltip(move |window, cx| {
            let text = download.describe(Instant::now());
            #[cfg(test)]
            TOOLTIP.with(|tooltip| *tooltip.borrow_mut() = Some(text.clone()));
            Tooltip::new(text).build(window, cx)
        })
}

/// The "Don't show again" check box: checked turns update checks off, unchecked turns them back
/// to `before`, the mode when the window opened.
fn dont_show_again(checked: bool, before: UpdateMode, window: &mut Window, cx: &mut App) {
    let mode = match (checked, before) {
        (true, _) => UpdateMode::Off,
        (false, UpdateMode::Off) => UpdateMode::Notify,
        (false, before) => before,
    };
    let value = toml::Value::try_from(mode).expect("a mode is a string");
    if let Err(error) = crate::preferences::set("updates.mode", Some(value), cx) {
        report_error(&error, window, cx);
    }
}

/// Redraws the windows while a download runs, so that its bars move.
fn redraw_while_downloading(cx: &mut App) {
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(REDRAW).await;
            let downloading = cx.update(|cx| {
                let downloading = Updates::download(cx).is_some();
                if downloading {
                    cx.refresh_windows();
                }
                downloading
            });
            if !downloading {
                break;
            }
        }
    })
    .detach();
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
            t!(
                "updates.newest-is",
                version = VERSION,
                channel = channel,
                newest = newest
            )
        }
        None => t!(
            "updates.none-published",
            version = VERSION,
            channel = channel
        ),
    };
    show_message(t!("updates.up-to-date"), vec![line], None, window, cx);
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
    let version = release.version.clone();
    let page = release.page.clone();
    let lines = [
        t!(
            "updates.available-line",
            newest = release.version,
            version = VERSION
        ),
        if installable {
            t!("updates.how-it-updates")
        } else {
            t!("updates.download-page", page = release.page)
        },
    ];
    window.open_dialog(cx, move |dialog, _, cx| {
        let body = div()
            .flex()
            .flex_col()
            .gap_2()
            .children(lines.iter().map(|line| div().child(line.clone())))
            .children(failure(&version, cx));
        let dialog = dialog
            .title(t!("updates.available"))
            .w(px(520.))
            .child(body)
            .on_close(|_, _, cx| forget_window(true, cx));
        if let Some(download) = downloading(&version, cx) {
            return dialog
                .footer(DialogFooter::new().child(download_bar(&download, "update-progress")))
                .on_ok(|_, _, _| false);
        }
        let label = if installable {
            "updates.update-and-restart"
        } else {
            "updates.open-download-page"
        };
        let workspace = workspace.clone();
        let page = page.clone();
        dialog
            .footer(
                DialogFooter::new()
                    .child(dialog_close("help.not-now"))
                    .child(dialog_action(
                        Button::new("action").primary().label(t!(label)),
                    )),
            )
            .on_ok(move |_, window, cx| {
                if !installable {
                    cx.open_url(&page);
                    return true;
                }
                workspace
                    .update(cx, |workspace, cx| {
                        let shown = Some(UpdateWindow::Dialog);
                        if let Err(error) = workspace.update_now(shown, window, cx) {
                            report_error(&error, window, cx);
                        }
                    })
                    .ok();
                // Open with the bar while the update downloads.
                Updates::download(cx).is_none()
            })
    });
}

fn show_ready(
    version: &Version,
    workspace: WeakEntity<Workspace>,
    window: &mut Window,
    cx: &mut App,
) {
    crate::help::show_message_with(
        t!("updates.ready"),
        vec![t!("updates.ready-line", version = version)],
        Some(crate::help::MessageAction::Run(
            "updates.restart-now",
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
/// Download (its page), and "Don't show again". Update keeps it open with the download's bar in
/// place of the buttons.
fn announce_available(
    release: &Release,
    installable: bool,
    workspace: WeakEntity<Workspace>,
    window: &mut Window,
    cx: &mut App,
) {
    let version = release.version.clone();
    let page = release.page.clone();
    let before = AppState::global(cx).settings.updates.mode;
    let notification = Notification::new()
        .title(t!("updates.is-available", version = release.version))
        .autohide(false)
        .on_close(|_, cx| forget_window(false, cx))
        .content(move |_, _, cx| {
            let notification = cx.entity().downgrade();
            let bottom = match downloading(&version, cx) {
                Some(download) => download_bar(&download, "announce-progress"),
                None => {
                    let off = AppState::global(cx).settings.updates.mode == UpdateMode::Off;
                    let button = if installable {
                        let workspace = workspace.clone();
                        Button::new("update")
                            .debug_selector(|| "announce-update".into())
                            .primary()
                            .small()
                            .label(t!("updates.update"))
                            .on_click(move |_, window, cx| {
                                let shown = Some(UpdateWindow::Notification(notification.clone()));
                                workspace
                                    .update(cx, |workspace, cx| {
                                        if let Err(error) = workspace.update_now(shown, window, cx)
                                        {
                                            report_error(&error, window, cx);
                                        }
                                    })
                                    .ok();
                            })
                    } else {
                        let page = page.clone();
                        Button::new("download")
                            .debug_selector(|| "announce-download".into())
                            .primary()
                            .small()
                            .label(t!("updates.download"))
                            .on_click(move |_, window, cx| {
                                notification.update(cx, |n, cx| n.dismiss(window, cx)).ok();
                                cx.open_url(&page);
                            })
                    };
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div().debug_selector(|| "dont-show-again".into()).child(
                                Checkbox::new("updates-dont-show-again")
                                    .label(t!("updates.dont-show-again"))
                                    .checked(off)
                                    .on_click(move |checked, window, cx| {
                                        dont_show_again(*checked, before, window, cx);
                                    }),
                            ),
                        )
                        .child(div().flex_1())
                        .child(button)
                }
            };
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(t!("updates.you-have", version = VERSION))
                .children(failure(&version, cx))
                .child(bottom)
                .into_any_element()
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
        .title(t!("updates.is-ready", version = version))
        .message(t!("updates.installed-on-restart"))
        .action(move |_, _, cx| {
            let notification = cx.entity().downgrade();
            let workspace = workspace.clone();
            Button::new("restart")
                .primary()
                .label(t!("updates.restart-now"))
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
        return t!("updates.turned-off");
    }
    let channel = crate::help::channel_name(settings.channel);
    let source = birchpad_update::manifest_url(settings);
    let how = if !checks_in_background() {
        t!("updates.how-manual")
    } else if settings.mode == UpdateMode::Auto && Updates::installed(cx) {
        t!("updates.how-auto")
    } else if Updates::installed(cx) {
        t!("updates.how-notify")
    } else {
        t!("updates.how-page")
    };
    let last = match state.state.updates.last_check {
        0 => String::new(),
        time => t!("updates.last-checked", date = manifest::date(time)),
    };
    t!(
        "updates.describe",
        channel = channel,
        source = source,
        how = how,
        last = last
    )
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead as _, BufReader, Write as _};
    use std::net::TcpListener;
    use std::path::Path;
    use std::sync::Mutex;

    use birchpad_commands::Invocation;
    use birchpad_config::UpdateChannel;
    use birchpad_update::manifest::{PRODUCT, SCHEMA};
    use birchpad_update::{Manifest, ManifestRelease, Package, SigningKey};
    use gpui_kit::{Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};
    use sha2::{Digest as _, Sha256};

    use super::*;
    use crate::workspace::tests::{click_on, open_workspace};

    const MB: u64 = 1024 * 1024;

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
            let package = Package {
                platform: "windows-x64".into(),
                file: format!("Birchpad-{version}-full.nupkg"),
                url: format!("https://example.com/{version}.nupkg"),
                size: 1,
                sha256: String::new(),
                sha1: String::new(),
            };
            self.announce_package(version, timestamp, package);
        }

        fn announce_package(&self, version: &str, timestamp: u64, package: Package) {
            let manifest = Manifest {
                schema: SCHEMA,
                product: PRODUCT.into(),
                timestamp,
                expires: timestamp + 30 * 86_400,
                releases: vec![ManifestRelease {
                    version: Version::parse(version).unwrap(),
                    page: format!("https://example.com/{version}"),
                    packages: vec![package],
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

        /// Packages come from a server, over HTTP.
        fn download(
            &self,
            url: &str,
            to: &Path,
            limit: u64,
            progress: &dyn Fn(u64),
        ) -> Result<(), String> {
            birchpad_update::HttpTransport::new("Birchpad (test)")
                .download(url, to, limit, progress)
        }
    }

    /// Answers one request with `body`, sent in `pieces` pieces.
    fn serve(body: Vec<u8>, pieces: usize) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            for line in BufReader::new(&stream).lines() {
                if line.unwrap().is_empty() {
                    break;
                }
            }
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            for piece in body.chunks(body.len().div_ceil(pieces)) {
                stream.write_all(piece).unwrap();
                stream.flush().unwrap();
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        format!("http://{address}/Birchpad-99.0.0-full.nupkg")
    }

    #[derive(Default)]
    struct FakeInstaller {
        calls: Mutex<Vec<String>>,
        /// Downloads the package from the transport and checks it, as Velopack's feed does,
        /// instead of pretending to.
        fetch: bool,
        /// Why downloads fail.
        fails: Mutex<Option<String>>,
        /// The bytes received, as reported.
        reports: Mutex<Vec<u64>>,
    }

    impl Installer for FakeInstaller {
        fn download(
            &self,
            version: &Version,
            package: &Package,
            transport: Arc<dyn Transport>,
            progress: birchpad_update::install::Progress,
        ) -> Result<(), String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("download {version}"));
            if let Some(error) = self.fails.lock().unwrap().clone() {
                return Err(error);
            }
            if !self.fetch {
                progress(package.size / 2);
                progress(package.size);
                return Ok(());
            }
            let dir = tempfile::tempdir().map_err(|error| error.to_string())?;
            let to = dir.path().join(&package.file);
            let report = |bytes| {
                self.reports.lock().unwrap().push(bytes);
                progress(bytes);
            };
            transport.download(&package.url, &to, package.size, &report)?;
            birchpad_update::verify_package(&to, package)
        }

        fn restart(&self) -> Result<(), String> {
            self.calls.lock().unwrap().push("restart".into());
            Ok(())
        }
    }

    fn install(installed: bool, cx: &mut VisualTestContext) -> (Arc<Fake>, Arc<FakeInstaller>) {
        install_with(FakeInstaller::default(), installed, cx)
    }

    fn install_with(
        installer: FakeInstaller,
        installed: bool,
        cx: &mut VisualTestContext,
    ) -> (Arc<Fake>, Arc<FakeInstaller>) {
        let fake = Arc::new(Fake {
            manifest: Mutex::default(),
            requests: Mutex::new(0),
        });
        fake.announce("99.0.0", now());
        let installer = Arc::new(installer);
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

    fn redraw(cx: &mut VisualTestContext) {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }

    fn drawn(selector: &'static str, cx: &mut VisualTestContext) -> bool {
        redraw(cx);
        cx.debug_bounds(selector).is_some()
    }

    #[track_caller]
    fn click(selector: &'static str, cx: &mut VisualTestContext) {
        redraw(cx);
        click_on(selector, MouseButton::Left, cx);
        cx.run_until_parked();
    }

    /// How full the bar `id` is drawn, from 0 to 1.
    #[track_caller]
    fn drawn_share(id: &'static str, cx: &mut VisualTestContext) -> f32 {
        redraw(cx);
        let track = match id {
            "update-progress" => "update-progress-track",
            "announce-progress" => "announce-progress-track",
            _ => "status-update-track",
        };
        let fill = match id {
            "update-progress" => "update-progress-fill",
            "announce-progress" => "announce-progress-fill",
            _ => "status-update-fill",
        };
        let track = cx.debug_bounds(track).expect("the bar is drawn");
        let fill = cx.debug_bounds(fill).expect("the bar is drawn");
        f32::from(fill.size.width) / f32::from(track.size.width)
    }

    /// A download of 99.0.0 of `size` bytes in progress, shown by `window`, as if the installer
    /// were busy with it.
    fn start(size: u64, window: Option<UpdateWindow>, cx: &mut VisualTestContext) -> Download {
        let download = Download::new(Version::new(99, 0, 0), size, window);
        cx.update(|_, cx| cx.global_mut::<Updates>().download = Some(download.clone()));
        download
    }

    fn finish(cx: &mut VisualTestContext) {
        cx.update(|_, cx| cx.global_mut::<Updates>().download = None);
    }

    fn calls(installer: &FakeInstaller) -> Vec<String> {
        installer.calls.lock().unwrap().clone()
    }

    /// A user settings file to write to, as Preferences does.
    fn settings_file(cx: &mut VisualTestContext) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        std::fs::write(&path, "").unwrap();
        let file = path.clone();
        cx.update(|_, cx| cx.global_mut::<AppState>().paths.user_settings = Some(file));
        (dir, path)
    }

    fn mode(cx: &mut VisualTestContext) -> UpdateMode {
        cx.update(|_, cx| AppState::global(cx).settings.updates.mode)
    }

    #[test]
    fn downloads_report_their_progress_and_stalls() {
        let download = Download::new(Version::new(99, 0, 0), 10 * MB, None);
        assert_eq!((download.received(), download.fraction()), (0, 0.));
        let progress = download.progress();
        progress(MB * 5 / 2);
        assert_eq!(download.fraction(), 0.25);
        assert_eq!(
            download.describe(Instant::now()),
            "Downloading Birchpad 99.0.0: 2.5 MB of 10.0 MB"
        );
        // More than the manifest's size is shown full (the installer refuses it).
        progress(20 * MB);
        assert_eq!((download.received(), download.fraction()), (10 * MB, 1.));

        // Stalled from ten seconds without bytes, not before.
        let now = Instant::now();
        assert_eq!(download.stalled(now + Duration::from_secs(9)), None);
        assert!(download.stalled(now + SHOWN_STALLED).is_some());
        assert_eq!(
            download.describe(now + Duration::from_secs(15)),
            "Birchpad 99.0.0: the download has stalled, nothing arrived for 15 s"
        );
        // Bytes arriving end the stall.
        progress(10 * MB);
        assert_eq!(download.stalled(Instant::now()), None);

        // An empty package is not divided by.
        let empty = Download::new(Version::new(99, 0, 0), 0, None);
        empty.progress()(0);
        assert_eq!(empty.fraction(), 0.);
        assert_eq!(
            empty.describe(Instant::now()),
            "Downloading Birchpad 99.0.0: 0.0 MB of 0.0 MB"
        );
        assert!(empty.in_status_bar());
        let shown = Download::new(Version::new(99, 0, 0), 0, Some(UpdateWindow::Dialog));
        assert!(!shown.in_status_bar());
    }

    #[gpui_kit::test]
    fn the_update_window_installs_from_a_server(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        let body: Vec<u8> = (0..300_000u32).map(|byte| byte as u8).collect();
        let fetching = FakeInstaller {
            fetch: true,
            ..FakeInstaller::default()
        };
        let (fake, installer) = install_with(fetching, true, cx);
        let package = Package {
            platform: "windows-x64".into(),
            file: "Birchpad-99.0.0-full.nupkg".into(),
            url: serve(body.clone(), 10),
            size: body.len() as u64,
            sha256: birchpad_update::hex(&Sha256::digest(&body)),
            sha1: String::new(),
        };
        fake.announce_package("99.0.0", now(), package);
        check(&workspace, Trigger::Request, cx);
        assert!(dialog_open(cx));
        assert!(drawn("dialog-action", cx));
        // Update and Restart.
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(calls(&installer), ["download 99.0.0", "restart"]);
        let reports = installer.reports.lock().unwrap().clone();
        assert!(reports.len() > 1, "the bar moves: {reports:?}");
        assert!(reports.is_sorted());
        assert_eq!(reports.last(), Some(&300_000));
        assert!(!dialog_open(cx), "closed for the restart");
        assert_eq!(notifications(cx), 0, "no notification besides the window");
        assert!(cx.update(|_, cx| Updates::download(cx).is_none()));
    }

    #[gpui_kit::test]
    fn the_bar_follows_the_download_in_place_of_the_buttons(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        let (_, installer) = install(true, cx);
        check(&workspace, Trigger::Request, cx);
        let download = start(1_000, Some(UpdateWindow::Dialog), cx);
        let progress = download.progress();
        progress(250);
        assert!(
            !drawn("dialog-action", cx),
            "the bar is where the buttons were"
        );
        assert!((drawn_share("update-progress", cx) - 0.25).abs() < 0.01);
        progress(750);
        assert!((drawn_share("update-progress", cx) - 0.75).abs() < 0.01);
        assert!(!drawn("status-update-track", cx), "the window shows it");

        // Enter does nothing while it downloads.
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert!(dialog_open(cx));
        assert!(calls(&installer).is_empty());

        // Closed, the window leaves the bar to the status bar.
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(!dialog_open(cx));
        assert!((drawn_share("status-update", cx) - 0.75).abs() < 0.01);

        // Ready: the bar goes away.
        finish(cx);
        assert!(!drawn("status-update-track", cx));
    }

    #[gpui_kit::test]
    fn a_failed_download_leaves_everything_as_it_was_and_says_why(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        let (_, installer) = install(true, cx);
        *installer.fails.lock().unwrap() = Some("connection reset".into());
        check(&workspace, Trigger::Request, cx);
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(calls(&installer), ["download 99.0.0"]);
        assert!(dialog_open(cx), "the window stays");
        assert!(drawn("update-failed", cx), "and says why");
        assert!(drawn("dialog-action", cx), "with its buttons back");
        assert_eq!(notifications(cx), 0);
        cx.update(|_, cx| {
            let updates = cx.global::<Updates>();
            assert!(updates.download.is_none() && updates.ready.is_none());
            assert_eq!(
                updates.available.as_ref().map(|release| &release.version),
                Some(&Version::new(99, 0, 0))
            );
            assert_eq!(
                updates.failed,
                Some((Version::new(99, 0, 0), "connection reset".into()))
            );
        });

        // Again, once the network is back.
        *installer.fails.lock().unwrap() = None;
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(
            calls(&installer),
            ["download 99.0.0", "download 99.0.0", "restart"]
        );
        assert!(!dialog_open(cx));
        assert!(cx.update(|_, cx| cx.global::<Updates>().failed.is_none()));

        // Without a window (Help > Update and Restart), a notification says why.
        *installer.fails.lock().unwrap() = Some("disk full".into());
        cx.update(|_, cx| cx.global_mut::<Updates>().ready = None);
        workspace.update_in(cx, |workspace, window, cx| {
            let update = Invocation::new("help.update-now");
            workspace.dispatch(&update, window, cx).unwrap();
        });
        cx.run_until_parked();
        assert_eq!(notifications(cx), 1);
    }

    #[gpui_kit::test]
    fn the_status_bar_shows_a_download_no_window_shows(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        let (_, installer) = install(true, cx);
        assert!(!drawn("status-update-track", cx), "nothing to show");
        let download = start(4 * MB, None, cx);
        download.progress()(MB);
        assert!((drawn_share("status-update", cx) - 0.25).abs() < 0.01);

        // Hovering says what is happening.
        TOOLTIP.with(|tooltip| tooltip.take());
        let bar = cx.debug_bounds("status-update-track").unwrap();
        cx.simulate_mouse_move(bar.center(), None, Modifiers::none());
        cx.executor().advance_clock(Duration::from_secs(2));
        cx.run_until_parked();
        assert_eq!(
            TOOLTIP.with(|tooltip| tooltip.take()).as_deref(),
            Some("Downloading Birchpad 99.0.0: 1.0 MB of 4.0 MB")
        );
        finish(cx);

        // Automatic updates download by themselves; once ready, the bar is gone.
        set_mode(UpdateMode::Auto, cx);
        check(&workspace, Trigger::Background, cx);
        assert_eq!(calls(&installer), ["download 99.0.0"]);
        assert!(!drawn("status-update-track", cx));
        assert!(cx.update(|_, cx| cx.global::<Updates>().ready.is_some()));
    }

    #[gpui_kit::test]
    fn the_announcement_updates_in_place(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        let (_, installer) = install(true, cx);
        check(&workspace, Trigger::Background, cx);
        assert_eq!(notifications(cx), 1);
        assert!(drawn("announce-update", cx));
        assert!(drawn("dont-show-again", cx));

        // While it downloads, the bar is where the buttons were.
        let notification = cx.update(|window, cx| window.notifications(cx)[0].downgrade());
        let download = start(1_000, Some(UpdateWindow::Notification(notification)), cx);
        download.progress()(500);
        assert!((drawn_share("announce-progress", cx) - 0.5).abs() < 0.01);
        assert!(!drawn("announce-update", cx));
        assert!(!drawn("dont-show-again", cx));
        assert!(
            !drawn("status-update-track", cx),
            "the notification shows it"
        );
        finish(cx);

        click("announce-update", cx);
        assert_eq!(calls(&installer), ["download 99.0.0", "restart"]);
        assert!(!drawn("announce-progress-track", cx));
    }

    #[gpui_kit::test]
    fn dont_show_again_turns_checks_off_beside_update(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        let (_, installer) = install(true, cx);
        let (_dir, path) = settings_file(cx);
        check(&workspace, Trigger::Background, cx);
        click("dont-show-again", cx);
        assert_eq!(mode(cx), UpdateMode::Off);
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("mode = \"off\""),
            "saved"
        );
        let about = cx.update(|_, cx| describe(cx));
        assert_eq!(about, t!("updates.turned-off"), "as Help > About says");
        // Unchecked: as before.
        click("dont-show-again", cx);
        assert_eq!(mode(cx), UpdateMode::Notify);
        // Checked, the update can still be installed from the window.
        click("dont-show-again", cx);
        click("announce-update", cx);
        assert_eq!(calls(&installer), ["download 99.0.0", "restart"]);
        assert_eq!(mode(cx), UpdateMode::Off);
    }

    #[gpui_kit::test]
    fn dont_show_again_turns_checks_off_beside_download(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        install(false, cx);
        let (_dir, _path) = settings_file(cx);
        // Automatic, but a portable copy only announces.
        set_mode(UpdateMode::Auto, cx);
        check(&workspace, Trigger::Background, cx);
        click("dont-show-again", cx);
        assert_eq!(mode(cx), UpdateMode::Off);
        // Unchecked: back to automatic.
        click("dont-show-again", cx);
        assert_eq!(mode(cx), UpdateMode::Auto);
        click("announce-download", cx);
        assert_eq!(
            cx.opened_url().as_deref(),
            Some("https://example.com/99.0.0")
        );
    }

    #[gpui_kit::test]
    fn dont_show_again_reports_what_it_cannot_change(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        install(false, cx);
        let (_dir, path) = settings_file(cx);
        check(&workspace, Trigger::Background, cx);
        // Set by the administrator.
        cx.update(|_, cx| {
            cx.global_mut::<AppState>()
                .policies
                .push(("updates.mode".into(), "\"notify\"".into()));
        });
        click("dont-show-again", cx);
        assert_eq!(mode(cx), UpdateMode::Notify);
        assert_eq!(notifications(cx), 2, "the error");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
        // Turning it back on from off with no mode to go back to: notify.
        cx.update(|_, cx| {
            cx.global_mut::<AppState>().policies.clear();
            cx.global_mut::<AppState>().settings.updates.mode = UpdateMode::Off;
        });
        cx.update(|window, cx| dont_show_again(false, UpdateMode::Off, window, cx));
        assert_eq!(mode(cx), UpdateMode::Notify);
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

    /// What a server answers for a path: the body, or only its first bytes (the connection
    /// closes before the length it announced).
    type Route = (&'static str, Vec<u8>, Option<usize>);

    /// A server for every request, with the routes `routes` makes for its address.
    fn serve_paths(routes: impl FnOnce(&str) -> Vec<Route>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let routes = routes(&base);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else {
                    continue;
                };
                let mut first = String::new();
                for line in BufReader::new(&stream).lines() {
                    let line = line.unwrap_or_default();
                    if first.is_empty() {
                        first = line.clone();
                    }
                    if line.is_empty() {
                        break;
                    }
                }
                let path = first.split(' ').nth(1).unwrap_or_default().to_owned();
                match routes.iter().find(|(route, ..)| *route == path) {
                    Some((_, body, cut)) => {
                        write!(
                            stream,
                            "HTTP/1.1 200 OK
Content-Length: {}
Connection: close

",
                            body.len()
                        )
                        .ok();
                        stream.write_all(&body[..cut.unwrap_or(body.len())]).ok();
                    }
                    None => {
                        write!(
                            stream,
                            "HTTP/1.1 404 Not Found
Content-Length: 0
Connection: close

"
                        )
                        .ok();
                    }
                }
            }
        });
        base
    }

    /// A manifest announcing `version` with `package`, signed with `key`.
    fn manifest_of(version: &str, package: Package, signer: &SigningKey) -> Vec<u8> {
        let manifest = Manifest {
            schema: SCHEMA,
            product: PRODUCT.into(),
            timestamp: now(),
            expires: now() + 30 * 86_400,
            releases: vec![ManifestRelease {
                version: Version::parse(version).unwrap(),
                page: format!("https://example.com/{version}"),
                packages: vec![package],
            }],
        };
        serde_json::to_vec(&manifest::sign(&manifest, &[signer.clone()])).unwrap()
    }

    /// The package of `body`, served at `base`/p.nupkg.
    fn package_at(base: &str, body: &[u8]) -> Package {
        Package {
            platform: "windows-x64".into(),
            file: "Birchpad-99.0.0-full.nupkg".into(),
            url: format!("{base}/p.nupkg"),
            size: body.len() as u64,
            sha256: birchpad_update::hex(&Sha256::digest(body)),
            sha1: String::new(),
        }
    }

    /// Updates read from `base`/m.json over HTTP, installed with a fetching installer.
    fn over_http(base: &str, cx: &mut VisualTestContext) -> Arc<FakeInstaller> {
        let installer = Arc::new(FakeInstaller {
            fetch: true,
            ..FakeInstaller::default()
        });
        let as_installer: Arc<dyn Installer> = installer.clone();
        let url = format!("{base}/m.json");
        cx.update(|_, cx| {
            let transport = Arc::new(birchpad_update::HttpTransport::new("Birchpad (test)"));
            Updates::install(
                transport,
                vec![key().verifying_key()],
                Some(as_installer),
                cx,
            );
            cx.global_mut::<Updates>().platform = Some("windows-x64");
            let updates = &mut cx.global_mut::<AppState>().settings.updates;
            updates.mode = UpdateMode::Notify;
            updates.channel = UpdateChannel::Stable;
            updates.url = Some(url);
        });
        installer
    }

    fn available(cx: &mut VisualTestContext) -> Option<Version> {
        cx.update(|_, cx| {
            cx.global::<Updates>()
                .available
                .as_ref()
                .map(|release| release.version.clone())
        })
    }

    #[gpui_kit::test]
    fn a_local_server_offers_what_its_signed_manifest_says(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        let body = b"the package".to_vec();
        let other_key = SigningKey::from_bytes(&[3; 32]);
        // A newer release, then an older one, then one signed with a key not trusted.
        for (version, signer, offered) in [
            ("99.0.0", key(), true),
            ("0.0.1", key(), false),
            ("99.0.0", other_key, false),
        ] {
            let base = serve_paths(|base| {
                let manifest = manifest_of(version, package_at(base, &body), &signer);
                vec![
                    ("/m.json", manifest, None),
                    ("/p.nupkg", body.clone(), None),
                ]
            });
            over_http(&base, cx);
            check(&workspace, Trigger::Request, cx);
            assert!(dialog_open(cx), "{version}: a window answers");
            assert_eq!(
                available(cx).is_some(),
                offered,
                "{version} offered: {offered}"
            );
            cx.simulate_keystrokes("escape");
            cx.run_until_parked();
        }
        // Nobody listening: the check fails and says so.
        let address = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        over_http(&format!("http://{address}"), cx);
        check(&workspace, Trigger::Request, cx);
        assert!(dialog_open(cx), "Cannot Check for Updates");
        assert_eq!(available(cx), None);
    }

    #[gpui_kit::test]
    fn downloads_cut_short_or_tampered_with_install_nothing(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        let body: Vec<u8> = (0..50_000u32).map(|byte| byte as u8).collect();
        let mut tampered = body.clone();
        tampered[25_000] ^= 1;
        for (served, cut, why) in [
            (body.clone(), Some(10_000), ""),
            (tampered, None, "SHA-256"),
        ] {
            let base = serve_paths(|base| {
                let manifest = manifest_of("99.0.0", package_at(base, &body), &key());
                vec![("/m.json", manifest, None), ("/p.nupkg", served, cut)]
            });
            let installer = over_http(&base, cx);
            check(&workspace, Trigger::Request, cx);
            cx.simulate_keystrokes("enter");
            cx.run_until_parked();
            assert_eq!(calls(&installer), ["download 99.0.0"], "never restarted");
            assert!(dialog_open(cx), "the window stays and says why");
            let failed = cx.update(|_, cx| cx.global::<Updates>().failed.clone());
            let (version, error) = failed.expect("the download failed");
            assert_eq!(version, Version::new(99, 0, 0));
            assert!(error.contains(why), "{error}");
            assert!(cx.update(|_, cx| cx.global::<Updates>().ready.is_none()));
            cx.simulate_keystrokes("escape");
            cx.run_until_parked();
        }
    }
}
