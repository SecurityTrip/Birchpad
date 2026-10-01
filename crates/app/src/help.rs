//! The Help menu: About and Check for Updates.
//!
//! Checking for updates is the only thing in Birchpad that uses the network, and it happens
//! only when the user picks Help > Check for Updates. With `updates.mode = "off"` the command is
//! disabled and makes no request.

use std::fmt::Write as _;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use birchpad_config::{UpdateChannel, UpdateMode};
use birchpad_update::{CheckError, HttpTransport, Outcome, Transport};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::{DialogClose, DialogFooter};
use gpui_kit::{
    App, AppContext as _, ClipboardItem, Context, Global, SharedString, Window, div, prelude::*,
    px, rgb,
};

use crate::app_state::AppState;
use crate::commands::CommandRegistry;
use crate::workspace::Workspace;

pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");
/// "stable", "beta", "nightly" for release builds, "development" otherwise (see `build.rs`).
pub(crate) const BUILD_CHANNEL: &str = env!("BIRCHPAD_CHANNEL");
pub(crate) const COMMIT: Option<&str> = option_env!("BIRCHPAD_COMMIT");

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("help.about", |_, (), window, cx| {
        show_about(window, cx);
        Ok(())
    });
    registry.workspace("help.check-updates", |this, (), window, cx| {
        this.check_for_updates(window, cx)
    });
    registry.enabled_when("help.check-updates", |cx| {
        AppState::global(cx).settings.updates.mode != UpdateMode::Off
    });
}

/// How update checks reach the network; tests install a fake.
pub(crate) struct Updates {
    transport: Arc<dyn Transport>,
    checking: bool,
}

impl Global for Updates {}

impl Updates {
    pub(crate) fn install(transport: Arc<dyn Transport>, cx: &mut App) {
        cx.set_global(Self {
            transport,
            checking: false,
        });
    }

    /// The real transport. Creating it does not touch the network.
    pub(crate) fn http() -> Arc<dyn Transport> {
        let user_agent = format!("Birchpad/{VERSION} ({})", std::env::consts::OS);
        Arc::new(HttpTransport::new(&user_agent))
    }
}

fn channel_name(channel: UpdateChannel) -> &'static str {
    match channel {
        UpdateChannel::Stable => "stable",
        UpdateChannel::Beta => "beta",
        UpdateChannel::Nightly => "nightly",
    }
}

/// The rows of the About box: `(label, value)`; continuation rows have an empty label.
pub(crate) fn about_rows(cx: &App) -> Vec<(&'static str, String)> {
    let state = AppState::global(cx);
    let updates = &state.settings.updates;
    let commit = COMMIT.map_or("unknown", |commit| &commit[..commit.len().min(12)]);
    let location = |path: Option<&std::path::Path>| {
        path.map_or_else(|| "none".to_owned(), |path| path.display().to_string())
    };
    let mut rows = vec![
        ("Version", VERSION.to_owned()),
        ("Build", format!("{BUILD_CHANNEL}, commit {commit}")),
        (
            "Updates",
            match updates.mode {
                UpdateMode::Off => "turned off".to_owned(),
                _ => format!(
                    "{} channel from {}, checked only from Help > Check for Updates",
                    channel_name(updates.channel),
                    birchpad_update::feed_url(updates)
                ),
            },
        ),
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

/// A dialog with wrapped text and OK, or, given a `page`, Open Download Page and Not Now.
fn show_message(
    title: impl Into<SharedString>,
    lines: Vec<String>,
    page: Option<String>,
    window: &mut Window,
    cx: &mut App,
) {
    let title = title.into();
    window.open_dialog(cx, move |dialog, _, _| {
        let body = div()
            .flex()
            .flex_col()
            .gap_2()
            .children(lines.iter().map(|line| div().child(line.clone())));
        let dialog = dialog.title(title.clone()).w(px(520.)).child(body);
        match &page {
            Some(page) => {
                let page = page.clone();
                dialog
                    .footer(
                        DialogFooter::new()
                            .child(DialogClose::new().trigger(|button| button.label("Not Now")))
                            .child(crate::workspace::dialog_action(
                                Button::new("open").primary().label("Open Download Page"),
                            )),
                    )
                    .on_ok(move |_, _, cx| {
                        cx.open_url(&page);
                        true
                    })
            }
            None => dialog
                .footer(DialogFooter::new().child(crate::workspace::dialog_action(
                    Button::new("ok").primary().label("OK"),
                )))
                .on_ok(|_, _, _| true),
        }
    });
}

impl Workspace {
    /// Asks the release feed whether a newer version exists and offers to open its download
    /// page. Runs only on the user's request.
    pub(crate) fn check_for_updates(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let settings = AppState::global(cx).settings.updates.clone();
        if settings.mode == UpdateMode::Off {
            return Err(CheckError::Disabled.into());
        }
        let updates = cx
            .try_global::<Updates>()
            .context("checking for updates is not available")?;
        if updates.checking {
            return Ok(());
        }
        let transport = updates.transport.clone();
        cx.global_mut::<Updates>().checking = true;
        let current = semver::Version::parse(VERSION).expect("the package version is semver");
        let check = cx.background_spawn(async move {
            birchpad_update::check(&settings, &current, transport.as_ref())
        });
        let window = window.window_handle();
        cx.spawn(async move |_, cx| {
            let outcome = check.await;
            cx.update(|cx| {
                // Even if the window was closed meanwhile, so that the next check runs.
                cx.global_mut::<Updates>().checking = false;
                window
                    .update(cx, |_, window, cx| show_outcome(outcome, window, cx))
                    .ok();
            });
        })
        .detach();
        Ok(())
    }
}

fn show_outcome(outcome: Result<Outcome, CheckError>, window: &mut Window, cx: &mut App) {
    let channel = channel_name(AppState::global(cx).settings.updates.channel);
    match outcome {
        Ok(Outcome::Available(release)) => {
            let lines = vec![
                format!(
                    "Birchpad {} is available. You have version {VERSION}.",
                    release.version
                ),
                format!("Download page: {}", release.page),
            ];
            show_message("Update Available", lines, Some(release.page), window, cx);
        }
        Ok(Outcome::UpToDate { newest }) => {
            let line = match newest {
                Some(newest) => {
                    format!("You have version {VERSION}; the newest {channel} release is {newest}.")
                }
                None => {
                    format!("You have version {VERSION}; no {channel} release is published yet.")
                }
            };
            show_message("Birchpad Is Up to Date", vec![line], None, window, cx);
        }
        Err(error) => {
            let lines = vec![error.to_string()];
            show_message("Cannot Check for Updates", lines, None, window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use birchpad_commands::Invocation;
    use gpui_kit::TestAppContext;

    use super::*;
    use crate::workspace::tests::open_workspace;

    struct Fake {
        feed: &'static str,
        requests: Mutex<usize>,
    }

    impl Transport for Fake {
        fn get(&self, _: &str) -> Result<Vec<u8>, String> {
            *self.requests.lock().unwrap() += 1;
            Ok(self.feed.as_bytes().to_vec())
        }
    }

    fn fake(feed: &'static str, cx: &mut gpui_kit::VisualTestContext) -> Arc<Fake> {
        let fake = Arc::new(Fake {
            feed,
            requests: Mutex::new(0),
        });
        cx.update(|_, cx| Updates::install(fake.clone(), cx));
        fake
    }

    fn set_mode(mode: UpdateMode, cx: &mut gpui_kit::VisualTestContext) {
        cx.update(|_, cx| cx.global_mut::<AppState>().settings.updates.mode = mode);
    }

    fn dialog_open(cx: &mut gpui_kit::VisualTestContext) -> bool {
        cx.update(|window, cx| window.has_active_dialog(cx))
    }

    fn check(workspace: &gpui_kit::Entity<Workspace>, cx: &mut gpui_kit::VisualTestContext) {
        workspace.update_in(cx, |workspace, window, cx| {
            workspace
                .dispatch(&Invocation::new("help.check-updates"), window, cx)
                .unwrap();
        });
        cx.run_until_parked();
    }

    #[gpui_kit::test]
    fn offers_the_download_page_of_a_newer_release(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        let fake = fake(
            r#"[{"tag_name": "v99.0.0", "html_url": "https://example.com/v99"}]"#,
            cx,
        );
        check(&workspace, cx);
        assert!(dialog_open(cx));
        // Enter confirms: Open Download Page.
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert!(!dialog_open(cx));
        assert_eq!(cx.opened_url().as_deref(), Some("https://example.com/v99"));
        assert_eq!(*fake.requests.lock().unwrap(), 1);
    }

    #[gpui_kit::test]
    fn off_disables_the_command_and_makes_no_request(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        let fake = fake("[]", cx);
        set_mode(UpdateMode::Off, cx);
        let enabled = cx.update(|_, cx| {
            cx.global::<CommandRegistry>()
                .is_enabled("help.check-updates", cx)
        });
        assert!(!enabled);
        workspace.update_in(cx, |workspace, window, cx| {
            let result = workspace.dispatch(&Invocation::new("help.check-updates"), window, cx);
            assert!(result.is_err());
        });
        cx.run_until_parked();
        assert!(!dialog_open(cx));
        assert_eq!(*fake.requests.lock().unwrap(), 0);

        set_mode(UpdateMode::Notify, cx);
        check(&workspace, cx);
        assert!(dialog_open(cx));
        assert_eq!(cx.opened_url(), None);
        assert_eq!(*fake.requests.lock().unwrap(), 1);
    }

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
        assert!(dialog_open(cx));
    }
}
