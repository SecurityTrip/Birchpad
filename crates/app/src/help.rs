//! The Help menu, as Notepad++'s: Command Line Arguments..., Birchpad's home page, manual and
//! forum, Set Updater Proxy..., Debug Info... (to copy into a bug report) and About, and the
//! dialogs of Check for Updates (`updates.rs`).

use std::fmt::Write as _;

use birchpad_config::UpdateChannel;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Disableable as _, WindowExt as _};
use gpui_kit::{
    App, AppContext as _, ClipboardItem, Context, Entity, SharedString, Window, div, prelude::*, px,
};

use crate::app_state::AppState;
use crate::commands::{CommandRegistry, Register as _};

/// The version of this build: the workspace version, or a nightly version (see `build.rs`).
pub const VERSION: &str = env!("BIRCHPAD_VERSION");
/// "stable", "beta", "nightly" for release builds, "development" otherwise (see `build.rs`).
pub const BUILD_CHANNEL: &str = env!("BIRCHPAD_CHANNEL");
pub const COMMIT: Option<&str> = option_env!("BIRCHPAD_COMMIT");

/// Birchpad's home page: its repository.
pub const HOME_PAGE: &str = "https://github.com/SecurityTrip/Birchpad";
/// The manual: the README, which says how to install and use Birchpad.
pub const MANUAL: &str = "https://github.com/SecurityTrip/Birchpad#readme";
/// Where users talk: the repository's discussions.
pub const FORUM: &str = "https://github.com/SecurityTrip/Birchpad/discussions";

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("help.about", |_, (), window, cx| {
        show_about(window, cx);
        Ok(())
    });
    registry.workspace("help.command-line", |_, (), window, cx| {
        show_command_line(window, cx);
        Ok(())
    });
    for (id, page) in [
        ("help.home-page", HOME_PAGE),
        ("help.manual", MANUAL),
        ("help.forum", FORUM),
    ] {
        registry.workspace(id, move |_, (), _, cx| {
            cx.open_url(page);
            Ok(())
        });
    }
    registry.workspace("help.debug-info", |_, (), window, cx| {
        show_debug_info(window, cx);
        Ok(())
    });
    registry.workspace("help.updater-proxy", |_, (), window, cx| {
        open_proxy(window, cx);
        Ok(())
    });
    crate::updates::register_commands(registry);
}

/// Help > Command Line Arguments...: the options, each with what it does.
pub fn command_line_rows() -> Vec<(&'static str, String)> {
    birchpad_cli::OPTIONS
        .iter()
        .map(|option| {
            let key = format!("cli.{}", crate::i18n::slug(option.description));
            (option.syntax, crate::i18n::get_or(&key, option.description))
        })
        .collect()
}

fn show_command_line(window: &mut Window, cx: &mut App) {
    let rows = command_line_rows();
    window.open_dialog(cx, move |dialog, _, _| {
        let table = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().pb_2().child(t!("help.usage")))
            .children(rows.iter().map(|(syntax, description)| {
                div()
                    .flex()
                    .flex_row()
                    .gap_3()
                    .child(
                        div()
                            .w(px(120.))
                            .flex_none()
                            .font_family(crate::MONOSPACE)
                            .child(*syntax),
                    )
                    .child(div().flex_1().min_w_0().child(description.clone()))
            }))
            .child(
                div()
                    .pt_2()
                    .text_color(crate::theme::paint(crate::theme::ui().muted))
                    .child(t!("help.other-options-are-ignored")),
            );
        dialog
            .title(t!("help.command-line-arguments"))
            .w(px(640.))
            .child(table)
            .footer(DialogFooter::new().child(crate::workspace::dialog_action(
                Button::new("ok").primary().label(t!("common.ok")),
            )))
            .on_ok(|_, _, _| true)
    });
}

/// Help > Debug Info...: what a bug report needs to know of this Birchpad, as text: the About
/// box's, and the system, the program, its arguments, the language, the theme, the proxy and
/// the plugins.
pub fn debug_info(cx: &App) -> String {
    let settings = &AppState::global(cx).settings;
    let mut text = about_text(cx);
    let exe = std::env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let plugins: Vec<String> = cx
        .try_global::<crate::extensions::plugins::Plugins>()
        .map(|plugins| {
            plugins
                .entries
                .iter()
                .map(|entry| entry.id().to_owned())
                .collect()
        })
        .unwrap_or_default();
    let proxy = settings.updates.proxy.trim();
    let rows = [
        (
            "OS",
            format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        ),
        ("Program", exe),
        ("Arguments", arguments.join(" ")),
        ("Language", birchpad_i18n::language()),
        ("Theme", crate::editor::theme::current().name.clone()),
        (
            "Proxy",
            if proxy.is_empty() {
                t!("help.proxy-of-the-system")
            } else {
                proxy.to_owned()
            },
        ),
        (
            "Plugins",
            if plugins.is_empty() {
                t!("help.none")
            } else {
                plugins.join(", ")
            },
        ),
    ];
    for (label, value) in rows {
        let _ = write!(text, "\n{:<10}{value}", format!("{label}:"));
    }
    text
}

fn show_debug_info(window: &mut Window, cx: &mut App) {
    let text = debug_info(cx);
    window.open_dialog(cx, move |dialog, _, _| {
        let copied = text.clone();
        dialog
            .title(t!("help.debug-info"))
            .w(px(640.))
            .child(
                div()
                    .id("debug-info")
                    .font_family(crate::MONOSPACE)
                    .text_sm()
                    .children(
                        text.lines()
                            .map(|line| div().whitespace_nowrap().child(line.to_owned())),
                    ),
            )
            .footer(
                DialogFooter::new()
                    .child(
                        Button::new("copy-debug-info")
                            .label(t!("help.copy"))
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()));
                            }),
                    )
                    .child(crate::workspace::dialog_action(
                        Button::new("ok").primary().label(t!("common.ok")),
                    )),
            )
            .on_ok(|_, _, _| true)
    });
}

#[cfg(feature = "test-support")]
thread_local! {
    /// The proxy dialog opened last, for tests to drive.
    pub static PROXY: std::cell::RefCell<Option<gpui_kit::WeakEntity<ProxyDialog>>> =
        std::cell::RefCell::default();
}

/// Help > Set Updater Proxy...: the proxy updates go through.
pub struct ProxyDialog {
    pub input: Entity<InputState>,
    pub error: Option<String>,
}

fn open_proxy(window: &mut Window, cx: &mut App) {
    let state = AppState::global(cx);
    let current = state.settings.updates.proxy.clone();
    let locked = state.is_locked("updates.proxy");
    let dialog = cx.new(|cx| ProxyDialog {
        input: cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(current)
                .placeholder("http://proxy.example.com:8080")
        }),
        error: locked.then(|| t!("help.set-by-policy")),
    });
    #[cfg(feature = "test-support")]
    PROXY.with(|proxy| *proxy.borrow_mut() = Some(dialog.downgrade()));
    window.open_dialog(cx, move |modal, _, _| {
        let confirm = dialog.clone();
        modal
            .title(t!("help.set-updater-proxy"))
            .w(px(480.))
            .child(dialog.clone())
            .footer(
                DialogFooter::new()
                    .child(crate::workspace::dialog_close("common.cancel"))
                    .child(crate::workspace::dialog_action(
                        Button::new("proxy-ok")
                            .primary()
                            .label(t!("common.ok"))
                            .disabled(locked),
                    )),
            )
            .on_ok(move |_, _, cx| confirm.update(cx, |dialog, cx| dialog.apply(cx)))
    });
}

impl ProxyDialog {
    /// OK: the proxy written to the settings and used from now on; `false` (the dialog stays,
    /// saying why) when it is no proxy or cannot be written.
    pub fn apply(&mut self, cx: &mut Context<Self>) -> bool {
        let text = self.input.read(cx).value().trim().to_owned();
        let result = birchpad_update::proxy(&text)
            .map_err(|error| anyhow::anyhow!(error))
            .and_then(|_| {
                let value = (!text.is_empty()).then(|| toml::Value::String(text.clone()));
                crate::preferences::set("updates.proxy", value, cx)
            })
            .and_then(|()| crate::updates::Updates::use_proxy(&text, cx));
        match result {
            Ok(()) => true,
            Err(error) => {
                self.error = Some(format!("{error:#}"));
                cx.notify();
                false
            }
        }
    }
}

impl Render for ProxyDialog {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(t!("help.proxy-explained"))
            .child(Input::new(&self.input).id("proxy-input"))
            .children(self.error.clone().map(|error| {
                div()
                    .text_color(crate::theme::paint(crate::theme::ui().error))
                    .child(error)
            }))
    }
}

pub fn channel_name(channel: UpdateChannel) -> &'static str {
    match channel {
        UpdateChannel::Stable => "stable",
        UpdateChannel::Beta => "beta",
        UpdateChannel::Nightly => "nightly",
    }
}

pub fn about_rows(cx: &App) -> Vec<(&'static str, String)> {
    let state = AppState::global(cx);
    let commit = COMMIT.map_or("unknown", |commit| &commit[..commit.len().min(12)]);
    let location = |path: Option<&std::path::Path>| {
        path.map_or_else(|| t!("help.none"), |path| path.display().to_string())
    };
    let mut rows = vec![
        ("Version", VERSION.to_owned()),
        (
            "Build",
            t!("help.build", channel = BUILD_CHANNEL, commit = commit),
        ),
        (
            "Installed",
            if crate::updates::Updates::installed(cx) {
                t!("help.installed-by-installer")
            } else if state.paths.portable {
                t!("help.portable-copy")
            } else {
                t!("help.not-installed")
            },
        ),
        ("Updates", crate::updates::describe(cx)),
        (
            "Settings",
            format!(
                "{}{}",
                location(state.paths.user_settings.as_deref()),
                if state.paths.portable {
                    format!(" ({})", t!("help.portable"))
                } else {
                    String::new()
                }
            ),
        ),
        ("Data", location(state.paths.user_data.as_deref())),
    ];
    if state.policies.is_empty() {
        rows.push(("Policies", t!("help.none")));
    }
    for (index, (key, value)) in state.policies.iter().enumerate() {
        let label = if index == 0 { "Policies" } else { "" };
        rows.push((label, format!("{key} = {value}")));
    }
    rows
}

/// The About box as plain text, for the clipboard.
pub fn about_text(cx: &App) -> String {
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
                            .text_color(crate::theme::paint(crate::theme::ui().muted))
                            .child(if label.is_empty() {
                                String::new()
                            } else {
                                crate::i18n::get_or(
                                    &format!("help.about-{}", crate::i18n::slug(label)),
                                    label,
                                )
                            }),
                    )
                    .child(div().flex_1().min_w_0().child(value.clone()))
            }));
        dialog
            .title(t!("help.about-birchpad"))
            .w(px(600.))
            .child(table)
            .footer(
                DialogFooter::new()
                    .child(
                        Button::new("copy")
                            .label(t!("help.copy"))
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                            }),
                    )
                    .child(crate::workspace::dialog_action(
                        Button::new("ok").primary().label(t!("common.ok")),
                    )),
            )
            .on_ok(|_, _, _| true)
    });
}

/// What a message's button does when clicked.
pub type OnClick = Box<dyn Fn(&mut Window, &mut App)>;

/// What the button of a message does, besides closing it.
pub enum MessageAction {
    /// Opens a web page; the button's text is the key given.
    Open(&'static str, String),
    /// Runs a function; the button's text is the key given.
    Run(&'static str, OnClick),
}

/// A dialog with wrapped text and OK.
pub fn show_message(
    title: impl Into<SharedString>,
    lines: Vec<String>,
    page: Option<String>,
    window: &mut Window,
    cx: &mut App,
) {
    let action = page.map(|page| MessageAction::Open("updates.open-download-page", page));
    show_message_with(title, lines, action, window, cx);
}

/// A dialog with wrapped text and OK, or an action and Not Now.
pub fn show_message_with(
    title: impl Into<SharedString>,
    lines: Vec<String>,
    action: Option<MessageAction>,
    window: &mut Window,
    cx: &mut App,
) {
    let title: SharedString = title.into();
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
                        Button::new("ok").primary().label(t!("common.ok")),
                    )))
                    .on_ok(|_, _, _| true);
            }
        };
        let action = action.clone();
        dialog
            .footer(
                DialogFooter::new()
                    .child(crate::workspace::dialog_close("help.not-now"))
                    .child(crate::workspace::dialog_action(
                        Button::new("action").primary().label(t!(label)),
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
