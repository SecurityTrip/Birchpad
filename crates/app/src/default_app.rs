//! Birchpad as the default editor of text files (issue #32): offered once at startup, until the
//! user sets it up or says not to ask again (`files.offer-default-app`), and at any time with
//! Settings > Make Birchpad the Default Editor. The registering is
//! `birchpad_cli::associations`'; on Windows the user then picks Birchpad in Settings > Default
//! apps, which no program may do for them.

use anyhow::{Context as _, Result};
use gpui_kit::{App, AppContext as _, PromptLevel, Window};

use crate::app_state::AppState;
use crate::commands::{CommandRegistry, Register as _};
use crate::workspace::{report_error, report_success};

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("settings.default-app", |_, (), window, cx| {
        set_up(window, cx)
    });
    registry.enabled_when("settings.default-app", |_| system::supported());
}

/// At startup: asks whether Birchpad should open text files, unless it does, the user said not
/// to ask, or the platform leaves it to the user (macOS).
pub(crate) fn offer(window: &mut Window, cx: &mut App) {
    let state = AppState::global(cx);
    if !state.settings.files.offer_default_app
        || state.state.default_app_declined
        || !system::supported()
        || system::is_default("txt")
    {
        return;
    }
    let answers = [
        t!("default-app.set-up"),
        t!("default-app.not-now"),
        t!("default-app.do-not-ask-again"),
    ];
    let answers: Vec<&str> = answers.iter().map(String::as_str).collect();
    let detail = if cfg!(windows) {
        t!("default-app.windows-asks")
    } else {
        t!("default-app.text-files")
    };
    let answer = window.prompt(
        PromptLevel::Info,
        &t!("default-app.open-text-files-with-birchpad"),
        Some(&detail),
        &answers,
        cx,
    );
    let handle = window.window_handle();
    cx.spawn(async move |cx| match answer.await.ok() {
        Some(0) => {
            cx.update_window(handle, |_, window, cx| {
                if let Err(error) = set_up(window, cx) {
                    report_error(&error, window, cx);
                }
            })
            .ok();
        }
        Some(2) => {
            cx.update(|cx| {
                AppState::update_state(cx, |state, _| state.default_app_declined = true);
            });
        }
        _ => {}
    })
    .detach();
}

/// Registers Birchpad for the text file types; on Windows, opens Default apps for the user to
/// pick it.
fn set_up(window: &mut Window, cx: &mut App) -> Result<()> {
    let exe = std::env::current_exe().context("cannot find Birchpad's program")?;
    system::register(&exe)
        .map_err(|error| anyhow::anyhow!("{}", t!("default-app.cannot-register", error = error)))?;
    match birchpad_cli::associations::settings_page() {
        Some(page) => system::open(page, cx),
        None => report_success(t!("default-app.birchpad-opens-text-files"), window, cx),
    }
    Ok(())
}

/// The system's part; tests have one of their own.
#[cfg(not(test))]
mod system {
    use std::path::Path;

    pub(super) fn supported() -> bool {
        birchpad_cli::associations::supported()
    }

    pub(super) fn is_default(extension: &str) -> bool {
        birchpad_cli::associations::is_default(extension)
    }

    pub(super) fn register(exe: &Path) -> std::io::Result<()> {
        birchpad_cli::associations::register(exe)
    }

    pub(super) fn open(page: &str, cx: &mut gpui_kit::App) {
        cx.open_url(page);
    }
}

#[cfg(test)]
pub(crate) mod system {
    use std::cell::RefCell;
    use std::path::Path;

    /// What tests let the system say, and what Birchpad asked of it.
    #[derive(Debug, Default)]
    pub(crate) struct Fake {
        pub(crate) unsupported: bool,
        pub(crate) default: bool,
        pub(crate) refuse: bool,
        pub(crate) registered: usize,
        pub(crate) opened: Vec<String>,
    }

    thread_local! {
        pub(crate) static FAKE: RefCell<Fake> = RefCell::default();
    }

    pub(super) fn supported() -> bool {
        FAKE.with(|fake| !fake.borrow().unsupported)
    }

    pub(super) fn is_default(_: &str) -> bool {
        FAKE.with(|fake| fake.borrow().default)
    }

    pub(super) fn register(_: &Path) -> std::io::Result<()> {
        FAKE.with(|fake| {
            let mut fake = fake.borrow_mut();
            if fake.refuse {
                return Err(std::io::Error::other("access denied"));
            }
            fake.registered += 1;
            Ok(())
        })
    }

    pub(super) fn open(page: &str, _: &mut gpui_kit::App) {
        FAKE.with(|fake| fake.borrow_mut().opened.push(page.to_owned()));
    }
}

#[cfg(test)]
mod tests;
