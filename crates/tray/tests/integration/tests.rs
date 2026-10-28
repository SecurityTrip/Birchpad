//! The tray in tests: the platforms' own calls are faked (`test-support`).

use birchpad_tray::{self as tray, Start};
use raw_window_handle::{HandleError, HasWindowHandle, WindowHandle};

/// A window without a handle: the faked calls never ask for it.
struct NoWindow;

impl HasWindowHandle for NoWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        Err(HandleError::Unavailable)
    }
}

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
}

#[test]
fn minimizing_goes_to_the_tray_while_the_setting_is_on() {
    assert!(!tray::minimizes_to_tray(), "off until set");
    tray::apply(true);
    // Only where there is a tray.
    assert_eq!(tray::minimizes_to_tray(), tray::SUPPORTED);
    tray::apply(true);
    assert_eq!(
        tray::minimizes_to_tray(),
        tray::SUPPORTED,
        "set twice, still on"
    );
    tray::apply(false);
    assert!(!tray::minimizes_to_tray());
}

#[test]
fn the_window_is_watched_and_sent_to_the_tray() {
    assert!(!tray::WATCHED.with(std::cell::Cell::get));
    assert!(!tray::IN_TRAY.with(std::cell::Cell::get));
    tray::watch(&NoWindow).unwrap();
    assert!(tray::WATCHED.with(std::cell::Cell::get));
    assert!(
        !tray::IN_TRAY.with(std::cell::Cell::get),
        "watched, not sent"
    );
    tray::send(&NoWindow).unwrap();
    assert!(tray::IN_TRAY.with(std::cell::Cell::get));
}
