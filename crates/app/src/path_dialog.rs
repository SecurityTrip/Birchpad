//! Choosing files: the platform's Open and Save As dialogs, with a simple in-app fallback for
//! systems where the platform dialog is unavailable (Linux without an XDG desktop portal).

use std::path::{Path, PathBuf};

use futures::channel::oneshot;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::{AppContext as _, AsyncWindowContext, PathPromptOptions, Window, prelude::*};

#[cfg(feature = "test-support")]
thread_local! {
    /// Tests: the system's dialogs are not available, as on Linux without a portal.
    pub static NO_SYSTEM_DIALOG: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Why the system's dialogs are not there, before asking them; on a desktop they always are,
/// and only their answer says otherwise.
fn unavailable() -> Option<anyhow::Error> {
    #[cfg(feature = "test-support")]
    if NO_SYSTEM_DIALOG.get() {
        return Some(anyhow::anyhow!("no desktop portal"));
    }
    None
}

/// The system dialog's answer, unless it is not available.
async fn answer<T>(
    platform: oneshot::Receiver<anyhow::Result<T>>,
) -> Result<anyhow::Result<T>, oneshot::Canceled> {
    match unavailable() {
        Some(error) => Ok(Err(error)),
        None => platform.await,
    }
}

/// Asks for files to open. `None` if the user cancelled.
pub async fn ask_open(directory: PathBuf, cx: &mut AsyncWindowContext) -> Option<Vec<PathBuf>> {
    let platform = cx
        .update(|_, cx| {
            cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                multiple: true,
                prompt: None,
            })
        })
        .ok()?;
    match answer(platform).await {
        Ok(Ok(paths)) => paths,
        Ok(Err(error)) => {
            eprintln!("the system's file dialog is not available: {error:#}");
            let path = ask_in_app("Open", &directory.join(""), cx).await?;
            Some(vec![path])
        }
        Err(_) => None,
    }
}

/// Asks for a folder (Find in Files). `None` if the user cancelled.
pub async fn ask_folder(directory: PathBuf, cx: &mut AsyncWindowContext) -> Option<PathBuf> {
    let platform = cx
        .update(|_, cx| {
            cx.prompt_for_paths(PathPromptOptions {
                files: false,
                directories: true,
                multiple: false,
                prompt: None,
            })
        })
        .ok()?;
    match answer(platform).await {
        Ok(Ok(paths)) => paths?.into_iter().next(),
        Ok(Err(error)) => {
            eprintln!("the system's folder dialog is not available: {error:#}");
            ask_in_app("Choose Folder", &directory, cx).await
        }
        Err(_) => None,
    }
}

/// Asks where to save a file. `None` if the user cancelled.
pub async fn ask_save(
    directory: PathBuf,
    suggested_name: String,
    cx: &mut AsyncWindowContext,
) -> Option<PathBuf> {
    let platform = cx
        .update(|_, cx| cx.prompt_for_new_path(&directory, Some(&suggested_name)))
        .ok()?;
    match answer(platform).await {
        Ok(Ok(path)) => path,
        Ok(Err(error)) => {
            eprintln!("the system's file dialog is not available: {error:#}");
            ask_in_app("Save As", &directory.join(suggested_name), cx).await
        }
        Err(_) => None,
    }
}

/// A dialog with a path field.
async fn ask_in_app(
    title: &'static str,
    initial: &Path,
    cx: &mut AsyncWindowContext,
) -> Option<PathBuf> {
    ask_text(title, initial.display().to_string(), cx)
        .await
        .map(PathBuf::from)
}

/// A dialog with a text field, such as a name; the button is labelled `title`. `None` if the
/// user cancelled; an empty answer is not taken.
pub async fn ask_text(
    title: &'static str,
    initial: String,
    cx: &mut AsyncWindowContext,
) -> Option<String> {
    let (sender, receiver) = oneshot::channel();
    cx.update(|window, cx| open_text_dialog(title, initial, sender, window, cx))
        .ok()?;
    receiver.await.ok().flatten()
}

pub fn open_text_dialog(
    title: &'static str,
    initial: String,
    sender: oneshot::Sender<Option<String>>,
    window: &mut Window,
    cx: &mut gpui_kit::App,
) {
    let input = cx.new(|cx| InputState::new(window, cx).default_value(initial));
    let sender = std::rc::Rc::new(std::cell::RefCell::new(Some(sender)));
    let respond = move |answer: Option<String>| {
        if let Some(sender) = sender.borrow_mut().take() {
            let _ = sender.send(answer);
        }
    };
    let on_cancel = respond.clone();
    let field = input.clone();
    window.open_dialog(cx, move |dialog, _, _| {
        let input = input.clone();
        let respond = respond.clone();
        let on_cancel = on_cancel.clone();
        dialog
            .title(title)
            .w(gpui_kit::px(640.))
            .child(Input::new(&input).id("path"))
            .footer(
                DialogFooter::new()
                    .child(crate::workspace::dialog_close("common.cancel"))
                    .child(crate::workspace::dialog_action(
                        Button::new("ok").label(title),
                    )),
            )
            .on_ok(move |_, _, cx| {
                let value = input.read(cx).value().trim().to_owned();
                if value.is_empty() {
                    return false;
                }
                respond(Some(value));
                true
            })
            .on_close(move |_, _, _| on_cancel(None))
    });
    // Typing replaces the suggestion at once, as in Notepad++'s dialogs.
    field.update(cx, |input, cx| {
        input.focus(window, cx);
        input.select_all(window, cx);
    });
}
