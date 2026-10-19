use birchpad_config::InterfaceMode;
use birchpad_theme::{DARK, DEFAULT, Theme};

use super::interface;

#[test]
fn the_interface_apart_from_the_theme() {
    let light = Theme::builtin(DEFAULT).unwrap().ui;
    let dark = Theme::builtin(DARK).unwrap().ui;
    assert_eq!(interface(InterfaceMode::Theme, true, false), (true, None));
    assert_eq!(interface(InterfaceMode::Theme, false, true), (false, None));
    assert_eq!(
        interface(InterfaceMode::Dark, false, false),
        (true, Some(dark))
    );
    assert_eq!(
        interface(InterfaceMode::Light, true, true),
        (false, Some(light))
    );
    assert_eq!(
        interface(InterfaceMode::System, false, true),
        (true, Some(dark))
    );
    assert_eq!(
        interface(InterfaceMode::System, true, false),
        (false, Some(light))
    );
}

#[gpui_kit::test]
fn a_dark_interface_with_a_light_theme(cx: &mut gpui_kit::TestAppContext) {
    let (_workspace, cx) = crate::workspace::tests::open_workspace(cx);
    cx.update(|_, cx| {
        super::apply(Theme::builtin(DEFAULT).unwrap().clone(), cx);
        cx.global_mut::<crate::app_state::AppState>()
            .settings
            .appearance
            .interface = InterfaceMode::Dark;
        super::reapply(cx);
    });
    assert_eq!(crate::theme::ui(), Theme::builtin(DARK).unwrap().ui);
    assert_eq!(
        crate::theme::current().name,
        DEFAULT,
        "the editor keeps its theme"
    );
    assert_eq!(
        crate::theme::editor(),
        Theme::builtin(DEFAULT).unwrap().editor
    );
    cx.update(|_, cx| {
        cx.global_mut::<crate::app_state::AppState>()
            .settings
            .appearance
            .interface = InterfaceMode::Theme;
        super::reapply(cx);
    });
    assert_eq!(crate::theme::ui(), Theme::builtin(DEFAULT).unwrap().ui);
}
