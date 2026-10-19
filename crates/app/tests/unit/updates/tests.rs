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
        birchpad_update::HttpTransport::new("Birchpad (test)").download(url, to, limit, progress)
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
    serde_json::to_vec(&manifest::sign(&manifest, std::slice::from_ref(signer))).unwrap()
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
