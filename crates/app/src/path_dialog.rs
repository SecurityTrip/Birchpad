//! Choosing files: the platform's Open and Save As dialogs, with a simple in-app fallback for
//! systems where the platform dialog is unavailable (Linux without an XDG desktop portal).

use std::path::{Path, PathBuf};

use futures::channel::oneshot;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::{AppContext as _, AsyncWindowContext, PathPromptOptions, Window, prelude::*};

/// Asks for files to open. `None` if the user cancelled.
pub(crate) async fn ask_open(
    directory: PathBuf,
    cx: &mut AsyncWindowContext,
) -> Option<Vec<PathBuf>> {
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
    match platform.await {
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
pub(crate) async fn ask_folder(directory: PathBuf, cx: &mut AsyncWindowContext) -> Option<PathBuf> {
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
    match platform.await {
        Ok(Ok(paths)) => paths?.into_iter().next(),
        Ok(Err(error)) => {
            eprintln!("the system's folder dialog is not available: {error:#}");
            ask_in_app("Choose Folder", &directory, cx).await
        }
        Err(_) => None,
    }
}

/// Asks where to save a file. `None` if the user cancelled.
pub(crate) async fn ask_save(
    directory: PathBuf,
    suggested_name: String,
    cx: &mut AsyncWindowContext,
) -> Option<PathBuf> {
    let platform = cx
        .update(|_, cx| cx.prompt_for_new_path(&directory, Some(&suggested_name)))
        .ok()?;
    match platform.await {
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
pub(crate) async fn ask_text(
    title: &'static str,
    initial: String,
    cx: &mut AsyncWindowContext,
) -> Option<String> {
    let (sender, receiver) = oneshot::channel();
    cx.update(|window, cx| open_text_dialog(title, initial, sender, window, cx))
        .ok()?;
    receiver.await.ok().flatten()
}

fn open_text_dialog(
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
                    .child(crate::workspace::dialog_close("Cancel"))
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
}

#[cfg(test)]
mod tests {
    use gpui_kit::{MouseButton, TestAppContext, VisualTestContext};

    use super::*;
    use crate::workspace::tests::{click_on, open_workspace};

    fn open(initial: &str, cx: &mut VisualTestContext) -> oneshot::Receiver<Option<String>> {
        let (sender, receiver) = oneshot::channel();
        cx.update(|window, cx| {
            open_text_dialog("Save As", initial.to_owned(), sender, window, cx);
        });
        cx.run_until_parked();
        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
        receiver
    }

    #[gpui_kit::test]
    fn enter_answers_with_the_path(cx: &mut TestAppContext) {
        let (_workspace, cx) = open_workspace(cx);
        let mut receiver = open("C:/notes/a.txt", cx);
        cx.simulate_keystrokes("enter");
        assert_eq!(
            receiver.try_recv().unwrap(),
            Some(Some("C:/notes/a.txt".to_owned()))
        );
        assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
    }

    #[gpui_kit::test]
    fn escape_cancels(cx: &mut TestAppContext) {
        let (_workspace, cx) = open_workspace(cx);
        let mut receiver = open("C:/notes/a.txt", cx);
        cx.simulate_keystrokes("escape");
        assert_eq!(receiver.try_recv().unwrap(), Some(None));
    }

    #[gpui_kit::test]
    fn an_empty_path_is_not_an_answer(cx: &mut TestAppContext) {
        let (_workspace, cx) = open_workspace(cx);
        let mut receiver = open("   ", cx);
        cx.simulate_keystrokes("enter");
        assert_eq!(receiver.try_recv().unwrap(), None, "still asking");
        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
        cx.simulate_keystrokes("escape");
        assert_eq!(receiver.try_recv().unwrap(), Some(None));
    }

    #[gpui_kit::test]
    fn its_buttons_answer_or_cancel_on_a_click(cx: &mut TestAppContext) {
        let (_workspace, cx) = open_workspace(cx);
        let mut receiver = open("C:/notes/a.txt", cx);
        click_on("dialog-action", MouseButton::Left, cx);
        assert_eq!(
            receiver.try_recv().unwrap(),
            Some(Some("C:/notes/a.txt".to_owned()))
        );
        assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));

        // An empty path is no answer for the mouse either; Cancel then answers nothing.
        let mut receiver = open("", cx);
        click_on("dialog-action", MouseButton::Left, cx);
        assert_eq!(receiver.try_recv().unwrap(), None, "still asking");
        click_on("dialog-close", MouseButton::Left, cx);
        assert_eq!(receiver.try_recv().unwrap(), Some(None));
        assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));

        // Only the left button presses them.
        let mut receiver = open("C:/b.txt", cx);
        click_on("dialog-action", MouseButton::Right, cx);
        click_on("dialog-close", MouseButton::Middle, cx);
        assert_eq!(receiver.try_recv().unwrap(), None);
        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    }

    #[gpui_kit::test]
    fn spaces_around_the_path_are_dropped(cx: &mut TestAppContext) {
        let (_workspace, cx) = open_workspace(cx);
        let mut receiver = open("  D:/b.txt  ", cx);
        cx.simulate_keystrokes("enter");
        assert_eq!(
            receiver.try_recv().unwrap(),
            Some(Some("D:/b.txt".to_owned()))
        );
    }
}
