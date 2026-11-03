//! Closing to the tray, asked once (roadmap 7.4t): the tray is reached only by minimizing, and
//! people do not find Preferences > MISC > Minimize to system tray. Closing the main window
//! while `misc.close-to-tray` has no answer asks whether to go to the tray or to close, and
//! saves the answer; File > Exit always closes. Only where there is a tray.

use birchpad_config::CloseToTray;
use gpui_kit::{Context, PromptLevel, Window};
use toml::Value;

use crate::app_state::AppState;
use crate::workspace::Workspace;

/// What a request to close the main window does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Closing {
    /// The window closes, as before (unsaved changes asked about first).
    Close,
    /// The window goes to the tray.
    Tray,
    /// The user is asked, once.
    Ask,
}

/// What closing does with `setting`, where the tray is `supported`.
pub fn closing(setting: CloseToTray, supported: bool) -> Closing {
    match (setting, supported) {
        (_, false) | (CloseToTray::Close, true) => Closing::Close,
        (CloseToTray::Tray, true) => Closing::Tray,
        (CloseToTray::Ask, true) => Closing::Ask,
    }
}

/// The value saved for an answer.
fn value(answer: CloseToTray) -> Value {
    Value::String(
        match answer {
            CloseToTray::Ask => "ask",
            CloseToTray::Tray => "tray",
            CloseToTray::Close => "close",
        }
        .to_owned(),
    )
}

impl Workspace {
    /// The main window's close button: whether it closes now. It goes to the tray instead, or
    /// asks first, as `misc.close-to-tray` says.
    pub fn close_requested(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let setting = AppState::global(cx).settings.misc.close_to_tray;
        match closing(setting, crate::tray::SUPPORTED) {
            Closing::Close => true,
            Closing::Tray => {
                send_to_tray(window);
                false
            }
            Closing::Ask => {
                self.ask_close_to_tray(window, cx);
                false
            }
        }
    }

    /// Asks whether closing goes to the tray or closes, saves the answer and does it.
    fn ask_close_to_tray(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let answers = [
            t!("close-to-tray.minimize"),
            t!("close-to-tray.close"),
            t!("common.cancel"),
        ];
        let answers: Vec<&str> = answers.iter().map(String::as_str).collect();
        let answer = window.prompt(
            PromptLevel::Info,
            &t!("close-to-tray.question"),
            Some(&t!("close-to-tray.detail")),
            &answers,
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let chosen = match answer.await.ok() {
                Some(0) => CloseToTray::Tray,
                Some(1) => CloseToTray::Close,
                _ => return,
            };
            cx.update(|window, cx| {
                // Without a settings file to keep it in, the answer still counts this time.
                if let Err(error) =
                    crate::preferences::set("misc.close-to-tray", Some(value(chosen)), cx)
                {
                    eprintln!("close to tray: {error:#}");
                }
                match chosen {
                    CloseToTray::Tray => send_to_tray(window),
                    _ => {
                        this.update(cx, |this, cx| {
                            let ready = this.prepare_to_quit(window, cx);
                            cx.spawn_in(window, async move |_, cx| {
                                if ready.await {
                                    cx.update(|window, _| window.remove_window()).ok();
                                }
                            })
                            .detach();
                        })
                        .ok();
                    }
                }
            })
            .ok();
        })
        .detach();
    }
}

fn send_to_tray(window: &mut Window) {
    if let Err(error) = crate::tray::send(window) {
        eprintln!("tray: {error:#}");
        window.minimize_window();
    }
}
