use std::ffi::OsString;
use std::fs;

use anyhow::Result;
use birchpad_commands::Invocation;
use gpui_kit::{Entity, MouseButton, TestAppContext, VisualTestContext};

use super::*;
use crate::workspace::tests::{click_on, open_workspace, tab_names};

fn command(workspace: &Entity<Workspace>, id: &str, cx: &mut VisualTestContext) -> Result<()> {
    let result = workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&Invocation::new(id), window, cx)
    });
    cx.run_until_parked();
    result
}

fn modes(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> WindowModes {
    workspace.read_with(cx, |workspace, _| workspace.modes)
}

fn drawn(selector: &'static str, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    cx.debug_bounds(selector).is_some()
}

fn launched() -> Vec<Vec<OsString>> {
    LAUNCHED.with(|launched| std::mem::take(&mut *launched.borrow_mut()))
}

#[gpui_kit::test]
fn always_on_top_and_full_screen(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    command(&workspace, "view.always-on-top", cx).unwrap();
    assert!(modes(&workspace, cx).always_on_top);
    assert!(ON_TOP.with(|top| top.get()));
    assert_eq!(
        is_checked("view.always-on-top", modes(&workspace, cx)),
        Some(true)
    );
    command(&workspace, "view.always-on-top", cx).unwrap();
    assert!(!ON_TOP.with(|top| top.get()));

    cx.simulate_keystrokes("f11");
    assert!(modes(&workspace, cx).full_screen);
    assert!(cx.update(|window, _| window.is_fullscreen()));
    assert!(drawn("pane-0-tab-bar", cx), "full screen keeps the tabs");
    cx.simulate_keystrokes("f11");
    assert!(!cx.update(|window, _| window.is_fullscreen()));
    assert_eq!(
        is_checked("view.full-screen", modes(&workspace, cx)),
        Some(false)
    );
    assert_eq!(is_checked("file.new", modes(&workspace, cx)), None);
}

#[gpui_kit::test]
fn post_it_shows_the_text_alone(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    assert!(drawn("pane-0-tab-bar", cx));
    cx.simulate_keystrokes("f12");
    assert!(modes(&workspace, cx).post_it);
    assert!(!drawn("pane-0-tab-bar", cx));
    assert!(drawn("pane-content-0", cx), "the text stays");
    assert!(
        !cx.update(|window, _| window.is_fullscreen()),
        "not full screen"
    );
    // Typing still works; F12 again brings everything back.
    cx.simulate_input("note");
    cx.simulate_keystrokes("f12");
    assert!(!modes(&workspace, cx).post_it);
    assert!(drawn("pane-0-tab-bar", cx));
    assert_eq!(crate::workspace::tests::active_text(&workspace, cx), "note");
}

#[gpui_kit::test]
fn distraction_free_and_the_way_out(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let before = cx.debug_bounds("pane-content-0").unwrap();
    command(&workspace, "view.distraction-free", cx).unwrap();
    assert!(cx.update(|window, _| window.is_fullscreen()));
    assert!(!drawn("pane-0-tab-bar", cx));
    let column = cx.debug_bounds("pane-content-0").unwrap();
    assert!(
        column.size.width < before.size.width,
        "a column in the middle"
    );
    assert!(column.left() > before.left());

    // Post-It replaces it, and the button leaves both.
    command(&workspace, "view.post-it", cx).unwrap();
    let modes_now = modes(&workspace, cx);
    assert!(modes_now.post_it && !modes_now.distraction_free);
    assert!(!cx.update(|window, _| window.is_fullscreen()));
    click_on("leave-text-only", MouseButton::Left, cx);
    assert!(!modes(&workspace, cx).text_only());
    assert!(drawn("pane-0-tab-bar", cx));

    // Full screen stays full screen when Distraction Free ends.
    command(&workspace, "view.full-screen", cx).unwrap();
    command(&workspace, "view.distraction-free", cx).unwrap();
    command(&workspace, "view.distraction-free", cx).unwrap();
    assert!(cx.update(|window, _| window.is_fullscreen()));
}

#[gpui_kit::test]
fn documents_go_to_a_new_instance(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, "one\ntwo").unwrap();
    let (workspace, cx) = open_workspace(cx);

    // Untitled, or with unsaved changes: refused, nothing started.
    let error = command(&workspace, "view.open-in-new-instance", cx).unwrap_err();
    assert!(error.to_string().contains("new 1"), "{error}");
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(&path, window, cx)
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("down end");
    cx.simulate_input("!");
    let error = command(&workspace, "view.move-to-new-instance", cx).unwrap_err();
    assert!(error.to_string().contains("a.txt"), "{error}");
    assert!(launched().is_empty());
    cx.simulate_keystrokes(&crate::workspace::tests::secondary("z"));

    // Open: started at the caret, the tab stays; Move: the tab closes.
    let path = workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.read(cx).buffer.read(cx).path().unwrap().to_owned()
    });
    command(&workspace, "view.open-in-new-instance", cx).unwrap();
    assert_eq!(launched(), [instance_args(&path, 2, 4)]);
    assert_eq!(tab_names(&workspace, cx), ["a.txt"]);
    command(&workspace, "view.move-to-new-instance", cx).unwrap();
    assert_eq!(launched().len(), 1);
    assert_eq!(tab_names(&workspace, cx), ["new 1"]);
}

#[test]
fn a_new_instance_gets_its_own_window_without_the_session() {
    let args = instance_args(std::path::Path::new("-odd name.txt"), 1, 1);
    let args: Vec<String> = args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        args,
        [
            "-multiInst",
            "-nosession",
            "-n1",
            "-c1",
            "--",
            "-odd name.txt"
        ]
    );
    // What the new instance reads back.
    let line = birchpad_cli::CommandLine::parse(
        instance_args(std::path::Path::new("/tmp/x.txt"), 7, 3),
        std::path::Path::new("/"),
    );
    assert!(line.multi_instance && line.no_session);
    assert_eq!((line.line, line.column), (Some(7), Some(3)));
    assert_eq!(line.files.len(), 1);
}

/// The platform's own call refuses a window it does not know; on Windows, a handle that is no
/// window.
#[test]
fn always_on_top_refuses_what_is_not_a_window_of_the_platform() {
    use raw_window_handle::{RawWindowHandle, WebWindowHandle};
    let web = RawWindowHandle::Web(WebWindowHandle::new(1));
    assert!(super::native::set_always_on_top(web, true).is_err());
    assert!(super::native::set_always_on_top(web, false).is_err());
    #[cfg(windows)]
    {
        use raw_window_handle::Win32WindowHandle;
        let gone = Win32WindowHandle::new(std::num::NonZeroIsize::new(1).unwrap());
        let error = super::native::set_always_on_top(RawWindowHandle::Win32(gone), true);
        assert!(error.is_err(), "no window has that handle");
    }
}
