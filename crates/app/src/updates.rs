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
#[path = "../tests/unit/updates/tests.rs"]
mod tests;
