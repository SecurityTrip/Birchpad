//! Minimize to the system tray (roadmap 7.4), on Windows and macOS: the setting, and how the
//! window starts with `-systemtray`. The platforms' own calls are faked in tests.

use birchpad::app_state::AppState;
use birchpad::tray::{self, Start};
use gpui_kit::{TestAppContext, VisualTestContext};
use toml::Value;

use crate::workspace::open_workspace;

#[test]
fn systemtray_starts_in_the_tray_where_there_is_one() {
    assert_eq!(tray::start(true, true), Start::InTray);
    // Where there is none, minimized, as before.
    assert_eq!(tray::start(true, false), Start::Minimized);
    // Without the option, as usual, tray or not.
    assert_eq!(tray::start(false, true), Start::Normal);
    assert_eq!(tray::start(false, false), Start::Normal);
}

#[test]
fn the_tray_is_windows_and_macos_s() {
    assert_eq!(tray::SUPPORTED, cfg!(windows) || cfg!(target_os = "macos"));
    assert_eq!(
        birchpad::preferences::available_here("misc.minimize-to-tray"),
        tray::SUPPORTED
    );
    // Every other setting is available everywhere.
    assert!(birchpad::preferences::available_here(
        "misc.confirm-save-all"
    ));
    assert!(birchpad::preferences::available_here(""));
}

fn set(cx: &mut VisualTestContext, value: Option<Value>) -> anyhow::Result<()> {
    cx.update(|_, cx| birchpad::preferences::set("misc.minimize-to-tray", value, cx))
}

#[gpui_kit::test]
fn minimizing_goes_to_the_tray_as_preferences_say(cx: &mut TestAppContext) {
    let (_workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    cx.update(|_, cx| cx.global_mut::<AppState>().paths.user_settings = Some(path.clone()));
    assert!(!tray::minimizes_to_tray(), "off by default");
    set(cx, Some(Value::Boolean(true))).unwrap();
    assert_eq!(tray::minimizes_to_tray(), tray::SUPPORTED);
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("minimize-to-tray = true")
    );
    // Back to the default.
    set(cx, None).unwrap();
    assert!(!tray::minimizes_to_tray());
    // What is not a switch is refused, and changes nothing.
    assert!(set(cx, Some(Value::String("yes".into()))).is_err());
    assert!(!tray::minimizes_to_tray());
}

#[gpui_kit::test]
fn the_window_is_watched_and_sent_to_the_tray(cx: &mut TestAppContext) {
    let (_workspace, cx) = open_workspace(cx);
    cx.update(|window, _| {
        tray::watch(window).unwrap();
        tray::send(window).unwrap();
    });
    assert!(tray::WATCHED.with(std::cell::Cell::get));
    assert!(tray::IN_TRAY.with(std::cell::Cell::get));
}
