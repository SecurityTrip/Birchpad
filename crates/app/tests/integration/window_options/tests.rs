//! Preferences left from Notepad++ 8 for the window (roadmap 7.4b): the bars, the tab bar and
//! Dark Mode's tones.

use birchpad::app_state::AppState;
use birchpad::workspace::Workspace;
use birchpad_commands::Invocation;
use birchpad_config::{AppearanceSettings, DarkTone, InterfaceMode, TabSettings};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use crate::workspace::open_workspace;

fn drawn(selector: &'static str, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    cx.debug_bounds(selector).is_some()
}

fn tabs(cx: &mut VisualTestContext, change: impl FnOnce(&mut TabSettings)) {
    cx.update(|_, cx| change(&mut cx.global_mut::<AppState>().settings.tabs));
}

fn appearance(cx: &mut VisualTestContext, change: impl FnOnce(&mut AppearanceSettings)) {
    cx.update(|_, cx| change(&mut cx.global_mut::<AppState>().settings.appearance));
}

fn command(
    workspace: &Entity<Workspace>,
    id: &str,
    cx: &mut VisualTestContext,
) -> anyhow::Result<()> {
    let result = workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&Invocation::new(id), window, cx)
    });
    cx.run_until_parked();
    result
}

#[gpui_kit::test]
fn the_status_bar_and_the_menu_bar_hide(cx: &mut TestAppContext) {
    let (_workspace, cx) = open_workspace(cx);
    assert!(drawn("status-eol", cx));
    appearance(cx, |appearance| appearance.status_bar = false);
    assert!(!drawn("status-eol", cx));
    if !cfg!(target_os = "macos") {
        assert!(drawn("menu-title-0", cx));
        appearance(cx, |appearance| appearance.menu_bar = false);
        assert!(!drawn("menu-title-0", cx));
        // F10 opens it again, as long as a menu is open.
        cx.simulate_keystrokes("f10");
        assert!(drawn("menu-title-0", cx));
        cx.simulate_keystrokes("escape");
        assert!(!drawn("menu-title-0", cx));
    }
}

#[gpui_kit::test]
fn tab_buttons_follow_the_tab_bar_options(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    command(&workspace, "file.new", cx).unwrap();
    // The second tab is active; the first has its close button too.
    assert!(drawn("pane-0-close-tab-0", cx));
    tabs(cx, |tabs| tabs.inactive_buttons = false);
    assert!(!drawn("pane-0-close-tab-0", cx));
    assert!(drawn("pane-0-close-tab-1", cx));
    // A pin button on every tab, which pins it.
    tabs(cx, |tabs| {
        tabs.inactive_buttons = true;
        tabs.only_pinned_button = false;
    });
    assert!(drawn("pane-0-pin-tab-0", cx));
    // Pinning turned off: no pin, and Pin Tab is off too.
    tabs(cx, |tabs| tabs.pinning = false);
    assert!(!drawn("pane-0-pin-tab-0", cx));
    assert!(command(&workspace, "view.pin-tab", cx).is_err());
}

#[gpui_kit::test]
fn a_dark_tone_tints_the_dark_interface(cx: &mut TestAppContext) {
    let (_workspace, cx) = open_workspace(cx);
    appearance(cx, |appearance| appearance.interface = InterfaceMode::Dark);
    cx.update(|_, cx| birchpad::themes::reapply(cx));
    let black = birchpad::theme::ui().background;
    appearance(cx, |appearance| appearance.dark_tone = DarkTone::Blue);
    cx.update(|_, cx| birchpad::themes::reapply(cx));
    let blue = birchpad::theme::ui().background;
    assert_ne!(black, blue);
    let [r, _, b, _] = blue.0.to_be_bytes();
    assert!(b > r, "{blue}");
    // A light interface takes no tone.
    appearance(cx, |appearance| appearance.interface = InterfaceMode::Light);
    cx.update(|_, cx| birchpad::themes::reapply(cx));
    let light = birchpad::theme::ui().background;
    appearance(cx, |appearance| appearance.dark_tone = DarkTone::Black);
    cx.update(|_, cx| birchpad::themes::reapply(cx));
    assert_eq!(birchpad::theme::ui().background, light);
}
