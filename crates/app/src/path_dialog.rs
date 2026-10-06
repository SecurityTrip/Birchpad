//! Choosing files: the platform's Open and Save As dialogs, with a simple in-app fallback for
//! systems where the platform dialog is unavailable (Linux without an XDG desktop portal).

use std::path::{Path, PathBuf};

use futures::channel::oneshot;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::dialog::{DialogClose, DialogFooter};
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
    let (sender, receiver) = oneshot::channel();
    let initial = initial.display().to_string();
    cx.update(|window, cx| open_path_dialog(title, initial, sender, window, cx))
        .ok()?;
    receiver.await.ok().flatten()
}

fn open_path_dialog(
    title: &'static str,
    initial: String,
    sender: oneshot::Sender<Option<PathBuf>>,
    window: &mut Window,
    cx: &mut gpui_kit::App,
) {
    let input = cx.new(|cx| InputState::new(window, cx).default_value(initial));
    let sender = std::rc::Rc::new(std::cell::RefCell::new(Some(sender)));
    let respond = move |answer: Option<PathBuf>| {
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
                    .child(DialogClose::new().trigger(|button| button.label("Cancel")))
                    .child(crate::workspace::dialog_action(
                        Button::new("ok").label(title),
                    )),
            )
            .on_ok(move |_, _, cx| {
                let value = input.read(cx).value().trim().to_owned();
                if value.is_empty() {
                    return false;
                }
                respond(Some(PathBuf::from(value)));
                true
            })
            .on_close(move |_, _, _| on_cancel(None))
    });
}

#[cfg(test)]
mod tests {
    use gpui_kit::{TestAppContext, VisualTestContext};

    use super::*;
    use crate::workspace::tests::open_workspace;

    fn open(initial: &str, cx: &mut VisualTestContext) -> oneshot::Receiver<Option<PathBuf>> {
        let (sender, receiver) = oneshot::channel();
        cx.update(|window, cx| {
            open_path_dialog("Save As", initial.to_owned(), sender, window, cx);
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
            Some(Some(PathBuf::from("C:/notes/a.txt")))
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
    fn spaces_around_the_path_are_dropped(cx: &mut TestAppContext) {
        let (_workspace, cx) = open_workspace(cx);
        let mut receiver = open("  D:/b.txt  ", cx);
        cx.simulate_keystrokes("enter");
        assert_eq!(
            receiver.try_recv().unwrap(),
            Some(Some(PathBuf::from("D:/b.txt")))
        );
    }
}
