//! Telling the user something without interrupting them (notifications), and the buttons every
//! dialog's footer has.

use birchpad_i18n::t;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::dialog::{DialogAction, DialogClose};
use gpui_kit::component::notification::Notification;
use gpui_kit::{App, InteractiveElement as _, IntoElement, ParentElement as _, Window, div};

/// Shows an error to the user without interrupting them.
pub fn report_error(error: &anyhow::Error, window: &mut Window, cx: &mut App) {
    window.push_notification(Notification::error(birchpad_i18n::tr_error(error)), cx);
}

/// Tells the user that something went well, without interrupting them.
pub fn report_success(message: impl Into<String>, window: &mut Window, cx: &mut App) {
    window.push_notification(Notification::success(message.into()), cx);
}

/// Tells the user something, without interrupting them.
pub fn report_info(message: impl Into<String>, window: &mut Window, cx: &mut App) {
    let message = message.into();
    #[cfg(feature = "test-support")]
    INFO.with(|info| info.borrow_mut().push(message.clone()));
    window.push_notification(Notification::info(message), cx);
}

/// Shows a warning to the user without interrupting them.
pub fn report_warning(message: impl Into<String>, window: &mut Window, cx: &mut App) {
    window.push_notification(Notification::warning(message.into()), cx);
}

#[cfg(feature = "test-support")]
thread_local! {
    /// The messages shown with `report_info`, for tests to read.
    static INFO: std::cell::RefCell<Vec<String>> = std::cell::RefCell::default();
}

/// The messages shown with [`report_info`] on this thread since the last call.
#[cfg(feature = "test-support")]
pub fn take_info() -> Vec<String> {
    INFO.with(|info| std::mem::take(&mut *info.borrow_mut()))
}

/// The confirming button of a dialog footer, at its natural width (`DialogAction` alone fills
/// the footer).
pub fn dialog_action(button: Button) -> impl IntoElement {
    div()
        .debug_selector(|| "dialog-action".into())
        .child(DialogAction::new().child(button))
}

/// A button that closes a dialog without its action: `label` is the key of its text
/// (`common.cancel`).
pub fn dialog_close(label: &'static str) -> DialogClose {
    DialogClose::new().trigger(move |button| {
        button
            .label(t!(label))
            .debug_selector(|| "dialog-close".into())
    })
}
