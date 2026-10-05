//! The Help menu: About, and the dialogs of Check for Updates (`updates.rs`).

use std::fmt::Write as _;

use birchpad_config::UpdateChannel;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::{DialogClose, DialogFooter};
use gpui_kit::{App, ClipboardItem, SharedString, Window, div, prelude::*, px, rgb};

use crate::app_state::AppState;
use crate::commands::CommandRegistry;

/// The version of this build: the workspace version, or a nightly version (see `build.rs`).
pub(crate) const VERSION: &str = env!("BIRCHPAD_VERSION");
/// "stable", "beta", "nightly" for release builds, "development" otherwise (see `build.rs`).
pub(crate) const BUILD_CHANNEL: &str = env!("BIRCHPAD_CHANNEL");
pub(crate) const COMMIT: Option<&str> = option_env!("BIRCHPAD_COMMIT");

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("help.about", |_, (), window, cx| {
        show_about(window, cx);
        Ok(())
    });
    crate::updates::register_commands(registry);
}

pub(crate) fn channel_name(channel: UpdateChannel) -> &'static str {
    match channel {
        UpdateChannel::Stable => "stable",
        UpdateChannel::Beta => "beta",
        UpdateChannel::Nightly => "nightly",
    }
}

pub(crate) fn about_rows(cx: &App) -> Vec<(&'static str, String)> {
    let state = AppState::global(cx);
    let commit = COMMIT.map_or("unknown", |commit| &commit[..commit.len().min(12)]);
    let location = |path: Option<&std::path::Path>| {
        path.map_or_else(|| "none".to_owned(), |path| path.display().to_string())
    };
    let mut rows = vec![
        ("Version", VERSION.to_owned()),
        ("Build", format!("{BUILD_CHANNEL}, commit {commit}")),
        (
            "Installed",
            if crate::updates::Updates::installed(cx) {
                "for this user, by the installer".to_owned()
            } else if state.paths.portable {
                "portable copy".to_owned()
            } else {
                "not by Birchpad's installer".to_owned()
            },
        ),
        ("Updates", crate::updates::describe(cx)),
        (
            "Settings",
            format!(
                "{}{}",
                location(state.paths.user_settings.as_deref()),
                if state.paths.portable {
                    " (portable)"
                } else {
                    ""
                }
            ),
        ),
        ("Data", location(state.paths.user_data.as_deref())),
    ];
    if state.policies.is_empty() {
        rows.push(("Policies", "none".to_owned()));
    }
    for (index, (key, value)) in state.policies.iter().enumerate() {
        let label = if index == 0 { "Policies" } else { "" };
        rows.push((label, format!("{key} = {value}")));
    }
    rows
}

/// The About box as plain text, for the clipboard.
pub(crate) fn about_text(cx: &App) -> String {
    let mut text = "Birchpad".to_owned();
    for (label, value) in about_rows(cx) {
        let label = if label.is_empty() {
            String::new()
        } else {
            format!("{label}:")
        };
        let _ = write!(text, "\n{label:<10}{value}");
    }
    text
}

fn show_about(window: &mut Window, cx: &mut App) {
    let rows = about_rows(cx);
    let text = about_text(cx);
    window.open_dialog(cx, move |dialog, _, _| {
        let text = text.clone();
        let table = div()
            .flex()
            .flex_col()
            .gap_1()
            .children(rows.iter().map(|(label, value)| {
                div()
                    .flex()
                    .flex_row()
                    .gap_3()
                    .child(
                        div()
                            .w(px(72.))
                            .flex_none()
                            .text_color(rgb(0x57606a))
                            .child(*label),
                    )
                    .child(div().flex_1().min_w_0().child(value.clone()))
            }));
        dialog
            .title("About Birchpad")
            .w(px(600.))
            .child(table)
            .footer(
                DialogFooter::new()
                    .child(Button::new("copy").label("Copy").on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                    }))
                    .child(crate::workspace::dialog_action(
                        Button::new("ok").primary().label("OK"),
                    )),
            )
            .on_ok(|_, _, _| true)
    });
}

/// What a message's button does when clicked.
pub(crate) type OnClick = Box<dyn Fn(&mut Window, &mut App)>;

/// What the button of a message does, besides closing it.
pub(crate) enum MessageAction {
    /// Opens a web page.
    Open(&'static str, String),
    /// Runs a function.
    Run(&'static str, OnClick),
}

/// A dialog with wrapped text and OK.
pub(crate) fn show_message(
    title: impl Into<SharedString>,
    lines: Vec<String>,
    page: Option<String>,
    window: &mut Window,
    cx: &mut App,
) {
    let action = page.map(|page| MessageAction::Open("Open Download Page", page));
    show_message_with(title, lines, action, window, cx);
}

/// A dialog with wrapped text and OK, or an action and Not Now.
pub(crate) fn show_message_with(
    title: impl Into<SharedString>,
    lines: Vec<String>,
    action: Option<MessageAction>,
    window: &mut Window,
    cx: &mut App,
) {
    let title = title.into();
    let action = std::rc::Rc::new(action);
    window.open_dialog(cx, move |dialog, _, _| {
        let body = div()
            .flex()
            .flex_col()
            .gap_2()
            .children(lines.iter().map(|line| div().child(line.clone())));
        let dialog = dialog.title(title.clone()).w(px(520.)).child(body);
        let label = match action.as_ref() {
            Some(MessageAction::Open(label, _) | MessageAction::Run(label, _)) => *label,
            None => {
                return dialog
                    .footer(DialogFooter::new().child(crate::workspace::dialog_action(
                        Button::new("ok").primary().label("OK"),
                    )))
                    .on_ok(|_, _, _| true);
            }
        };
        let action = action.clone();
        dialog
            .footer(
                DialogFooter::new()
                    .child(DialogClose::new().trigger(|button| button.label("Not Now")))
                    .child(crate::workspace::dialog_action(
                        Button::new("action").primary().label(label),
                    )),
            )
            .on_ok(move |_, window, cx| {
                match action.as_ref() {
                    Some(MessageAction::Open(_, page)) => cx.open_url(page),
                    Some(MessageAction::Run(_, run)) => run(window, cx),
                    None => {}
                }
                true
            })
    });
}

#[cfg(test)]
mod tests {
    use birchpad_commands::Invocation;
    use birchpad_config::UpdateMode;
    use gpui_kit::TestAppContext;

    use super::*;
    use crate::workspace::tests::open_workspace;

    #[gpui_kit::test]
    fn about_lists_build_paths_and_policies(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        cx.update(|_, cx| {
            cx.global_mut::<AppState>().policies =
                vec![("updates.mode".to_owned(), "\"off\"".to_owned())];
            cx.global_mut::<AppState>().settings.updates.mode = UpdateMode::Off;
        });
        let text = cx.update(|_, cx| about_text(cx));
        assert!(
            text.starts_with(&format!("Birchpad\nVersion:  {VERSION}\nBuild:    ")),
            "{text}"
        );
        assert!(
            text.contains("\nInstalled:not by Birchpad's installer\n"),
            "{text}"
        );
        assert!(text.contains("\nUpdates:  turned off\n"), "{text}");
        assert!(
            text.ends_with("\nPolicies: updates.mode = \"off\""),
            "{text}"
        );

        workspace.update_in(cx, |workspace, window, cx| {
            workspace
                .dispatch(&Invocation::new("help.about"), window, cx)
                .unwrap();
        });
        cx.run_until_parked();
        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    }
}
