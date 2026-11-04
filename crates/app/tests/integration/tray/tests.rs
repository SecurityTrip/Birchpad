//! Minimize to the system tray (roadmap 7.4), on Windows and macOS, as the application uses it:
//! the setting in Preferences, and the window watched. The platforms' own calls are faked in
//! tests; `birchpad-tray` tests the rest.

use birchpad::app_state::AppState;
use birchpad::tray;
use gpui_kit::{TestAppContext, VisualTestContext};
use toml::Value;

use crate::workspace::open_workspace;

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

use birchpad::close_to_tray::{Closing, closing};
use birchpad_config::CloseToTray;

#[test]
fn closing_asks_once_where_there_is_a_tray() {
    assert_eq!(closing(CloseToTray::Ask, true), Closing::Ask);
    assert_eq!(closing(CloseToTray::Tray, true), Closing::Tray);
    assert_eq!(closing(CloseToTray::Close, true), Closing::Close);
    // Without a tray, closing closes, whatever the setting.
    for setting in [CloseToTray::Ask, CloseToTray::Tray, CloseToTray::Close] {
        assert_eq!(closing(setting, false), Closing::Close);
    }
    assert_eq!(
        birchpad::preferences::available_here("misc.close-to-tray"),
        tray::SUPPORTED
    );
}

/// Closes the main window as its close button does: whether it closes now.
fn close(
    workspace: &gpui_kit::Entity<birchpad::workspace::Workspace>,
    cx: &mut VisualTestContext,
) -> bool {
    let closes = workspace.update_in(cx, |workspace, window, cx| {
        workspace.close_requested(window, cx)
    });
    cx.run_until_parked();
    closes
}

#[gpui_kit::test]
fn the_first_close_asks_and_the_answer_is_kept(cx: &mut TestAppContext) {
    if !tray::SUPPORTED {
        return;
    }
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    cx.update(|_, cx| cx.global_mut::<AppState>().paths.user_settings = Some(path.clone()));
    assert!(!close(&workspace, cx), "not closed while it asks");
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Minimize to Tray");
    cx.run_until_parked();
    assert!(tray::IN_TRAY.with(std::cell::Cell::get));
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("close-to-tray = \"tray\"")
    );
    // Asked once: the next close goes to the tray without a question.
    tray::IN_TRAY.with(|in_tray| in_tray.set(false));
    assert!(!close(&workspace, cx));
    assert!(!cx.has_pending_prompt());
    assert!(tray::IN_TRAY.with(std::cell::Cell::get));
}

#[gpui_kit::test]
fn cancelling_the_question_changes_nothing(cx: &mut TestAppContext) {
    if !tray::SUPPORTED {
        return;
    }
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    cx.update(|_, cx| cx.global_mut::<AppState>().paths.user_settings = Some(path.clone()));
    assert!(!close(&workspace, cx));
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert!(!tray::IN_TRAY.with(std::cell::Cell::get));
    assert!(!path.exists(), "nothing saved");
    assert_eq!(
        cx.update(|_, cx| AppState::global(cx).settings.misc.close_to_tray),
        CloseToTray::Ask
    );
}

#[gpui_kit::test]
fn closing_closes_when_that_was_the_answer(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().settings.misc.close_to_tray = CloseToTray::Close;
    });
    assert!(close(&workspace, cx));
    assert!(!cx.has_pending_prompt());
    assert!(!tray::IN_TRAY.with(std::cell::Cell::get));
}
