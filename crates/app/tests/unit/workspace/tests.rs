use super::*;
use gpui_kit::{Modifiers, MouseButton, TestAppContext, VisualTestContext};

/// `secondary-<key>` in the spelling GPUI's test harness expects on this platform.
pub(crate) fn secondary(key: &str) -> String {
    let modifier = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    format!("{modifier}-{key}")
}

/// The keystroke that moves the caret to the start of the document on this platform:
/// Ctrl+Home on Windows and Linux, Cmd+Up on macOS (Cmd+Home does nothing there).
pub(crate) fn document_start() -> &'static str {
    if cfg!(target_os = "macos") {
        "cmd-up"
    } else {
        "ctrl-home"
    }
}

/// Toggle Bookmark: Ctrl+F2, or Cmd+F2 on macOS.
pub(crate) fn toggle_bookmark() -> &'static str {
    if cfg!(target_os = "macos") {
        "cmd-f2"
    } else {
        "ctrl-f2"
    }
}

pub(crate) fn open_workspace(
    cx: &mut TestAppContext,
) -> (Entity<Workspace>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        // Dialogs open in place instead of sliding in over a quarter of a second.
        cx.set_reduce_motion(true);
        let mut settings = birchpad_config::resolve(birchpad_config::Sources::default());
        // Tests type brackets and press Enter as before auto-completion; its own tests turn
        // it on (`crate::editor::completion::tests`).
        settings.settings.completion.auto_close.clear();
        settings.settings.completion.words = false;
        cx.set_global(AppState::new(
            settings,
            birchpad_config::ConfigPaths::default(),
        ));
        crate::commands::init(None, cx);
    });
    // The production entry point, so the window has gpui-component's Root (notifications,
    // dialogs).
    let (window, workspace) = cx.update(|cx| {
        let options = gpui_kit::WindowOptions {
            window_bounds: Some(gpui_kit::WindowBounds::Windowed(
                gpui_kit::Bounds::maximized(None, cx),
            )),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| {
                let mut workspace = Workspace::new(window, cx);
                workspace.new_file(window, cx);
                workspace
            })
        })
        .expect("open test window")
    });
    let cx = VisualTestContext::from_window(window, cx).into_mut();
    // Active like the window the user works in: focus events are only reported then.
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    (workspace, cx)
}

pub(crate) fn tab_names(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    workspace.read_with(cx, |workspace, cx| {
        workspace
            .all_views(cx)
            .iter()
            .map(|view| view.read(cx).buffer.read(cx).display_name())
            .collect()
    })
}

/// The tab names of the main and the second view, and which view is active.
pub(crate) fn panes(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> ([Vec<String>; 2], usize) {
    workspace.read_with(cx, |workspace, cx| {
        let names = workspace.panes.clone().map(|pane| {
            pane.read(cx)
                .items()
                .iter()
                .map(|view| view.read(cx).buffer.read(cx).display_name())
                .collect()
        });
        (names, workspace.active_pane)
    })
}

/// The views of a pane.
pub(crate) fn pane_views(
    workspace: &Entity<Workspace>,
    pane: usize,
    cx: &mut VisualTestContext,
) -> Vec<Entity<EditorView>> {
    workspace.read_with(cx, |workspace, cx| {
        workspace.panes[pane].read(cx).items().to_vec()
    })
}

/// Sends a pane the event of a tab dropped on it.
pub(crate) fn drop_tab(
    workspace: &Entity<Workspace>,
    pane: usize,
    view: Entity<EditorView>,
    index: usize,
    cx: &mut VisualTestContext,
) {
    let pane = workspace.read_with(cx, |workspace, _| workspace.panes[pane].clone());
    pane.update(cx, |_, cx| {
        cx.emit(PaneEvent::Dropped {
            view,
            index,
            clone: false,
        })
    });
    cx.run_until_parked();
}

pub(crate) use birchpad_ui::take_info;

pub(crate) fn active_text(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.read(cx).text(cx).to_string()
    })
}

#[gpui_kit::test]
fn key_bindings_dispatch_commands_through_the_registry(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_keystrokes(&secondary("n"));
    cx.simulate_keystrokes(&secondary("n"));
    assert_eq!(tab_names(&workspace, cx), ["new 1", "new 2", "new 3"]);

    cx.simulate_input("abc");
    cx.simulate_keystrokes("left backspace");
    assert_eq!(active_text(&workspace, cx), "ac");
    cx.simulate_keystrokes(&secondary("z"));
    assert_eq!(active_text(&workspace, cx), "abc");

    // Ctrl+PageDown wraps around to the first tab; closing it frees the number 1 again.
    cx.simulate_keystrokes("ctrl-pagedown");
    assert_eq!(active_text(&workspace, cx), "");
    cx.simulate_keystrokes(&secondary("w"));
    assert_eq!(tab_names(&workspace, cx), ["new 2", "new 3"]);
    cx.simulate_keystrokes(&secondary("n"));
    assert_eq!(tab_names(&workspace, cx), ["new 2", "new 1", "new 3"]);
}

#[gpui_kit::test]
fn closing_the_last_tab_leaves_an_empty_document(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("text");
    workspace.update_in(cx, |workspace, window, cx| {
        workspace
            .dispatch(&Invocation::new("file.close-all"), window, cx)
            .unwrap();
    });
    cx.run_until_parked();
    cx.simulate_prompt_answer("Don't Save");
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx), ["new 1"]);
    assert_eq!(active_text(&workspace, cx), "");
}

/// Clicks the element drawn with this debug selector, with `button`, once it stands still:
/// the test window turns motion off, so dialogs open in place, but something that still
/// moves between finding it and clicking it would leave the mouse behind on a slow run.
#[track_caller]
pub(crate) fn click_on(selector: &'static str, button: MouseButton, cx: &mut VisualTestContext) {
    let Some(mut bounds) = cx.debug_bounds(selector) else {
        panic!("{selector} is not drawn");
    };
    for _ in 0..100 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        let Some(now) = cx.debug_bounds(selector) else {
            panic!("{selector} is no longer drawn");
        };
        if now == bounds {
            break;
        }
        bounds = now;
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let center = bounds.center();
    cx.simulate_mouse_down(center, button, Modifiers::none());
    cx.simulate_mouse_up(center, button, Modifiers::none());
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_close_button_of_a_tab_asks_about_unsaved_changes(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("changed");
    cx.simulate_keystrokes(&secondary("n"));

    // "new 1" has unsaved changes: it comes forward and is asked about.
    click_on("pane-0-close-tab-0", MouseButton::Left, cx);
    assert!(cx.has_pending_prompt());
    assert_eq!(active_text(&workspace, cx), "changed");
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx), ["new 1", "new 2"]);
    click_on("pane-0-close-tab-0", MouseButton::Left, cx);
    cx.simulate_prompt_answer("Don't Save");
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx), ["new 2"]);

    // Without changes, it closes at once.
    cx.simulate_keystrokes(&secondary("n"));
    assert_eq!(tab_names(&workspace, cx), ["new 2", "new 1"]);
    click_on("pane-0-close-tab-1", MouseButton::Left, cx);
    assert!(!cx.has_pending_prompt());
    assert_eq!(tab_names(&workspace, cx), ["new 2"]);
}

#[gpui_kit::test]
fn the_middle_mouse_button_closes_a_tab(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_keystrokes(&secondary("n"));
    cx.simulate_input("changed");
    cx.simulate_keystrokes(&secondary("n"));
    assert_eq!(tab_names(&workspace, cx), ["new 1", "new 2", "new 3"]);

    // A tab that is not the active one closes; the active one stays active.
    click_on("pane-0-tab-0", MouseButton::Middle, cx);
    assert_eq!(tab_names(&workspace, cx), ["new 2", "new 3"]);
    assert_eq!(active_text(&workspace, cx), "");

    // Unsaved changes are asked about, as with the close button.
    click_on("pane-0-tab-0", MouseButton::Middle, cx);
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Don't Save");
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx), ["new 3"]);

    // A left click only activates.
    cx.simulate_keystrokes(&secondary("n"));
    click_on("pane-0-tab-0", MouseButton::Left, cx);
    assert_eq!(tab_names(&workspace, cx), ["new 3", "new 1"]);
    let active = workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.read(cx).buffer.read(cx).display_name()
    });
    assert_eq!(active, "new 3");
}

#[gpui_kit::test]
fn command_line_opens_files_at_a_line_read_only(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a b.txt"), "one\n\ttwo\nthree\n").unwrap();
    std::fs::write(dir.path().join("other.txt"), "other").unwrap();
    let (workspace, cx) = open_workspace(cx);
    let caret = |workspace: &Entity<Workspace>, cx: &mut VisualTestContext| {
        workspace.update(cx, |workspace, cx| {
            let view = workspace.active_view(cx).unwrap();
            let info = view.update(cx, |view, cx| crate::status_bar::StatusInfo::of(view, cx));
            (info.line, info.column)
        })
    };
    let open = |args: &[&str], cx: &mut VisualTestContext| {
        let args = args.iter().map(std::ffi::OsString::from);
        let command_line = CommandLine::parse(args, dir.path());
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_command_line(&command_line, window, cx);
        });
        cx.run_until_parked();
    };

    // The caret is placed once the file has been read; -c counts tab stops like the status
    // bar's Col.
    open(&["-n2", "-c6", "-ro", "a b.txt", "other.txt"], cx);
    assert_eq!(tab_names(&workspace, cx), ["a b.txt", "other.txt"]);
    assert_eq!(caret(&workspace, cx), (1, 6));
    cx.simulate_input("x");
    assert_eq!(active_text(&workspace, cx), "other");
    cx.simulate_keystrokes("ctrl-shift-tab");
    assert_eq!(caret(&workspace, cx), (2, 6));

    // A later launch with an open file switches to its tab and moves the caret.
    open(&["-n3", "a b.txt"], cx);
    assert_eq!(caret(&workspace, cx), (3, 1));
    cx.simulate_input("x");
    assert_eq!(active_text(&workspace, cx), "one\n\ttwo\nthree\n");
    open(&["-p5", "other.txt"], cx);
    assert_eq!(caret(&workspace, cx), (1, 6));

    // Line numbers past the end go to the last line.
    open(&["-n99", "a b.txt"], cx);
    assert_eq!(caret(&workspace, cx), (4, 1));
}

#[gpui_kit::test]
fn unknown_and_pending_commands_fail_cleanly(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    workspace.update_in(cx, |workspace, window, cx| {
        let unknown = workspace.dispatch(&Invocation::new("no.such"), window, cx);
        assert!(unknown.is_err());
        let bad_args = workspace.dispatch(
            &Invocation::with_args("edit.insert-text", serde_json::json!({ "txt": 1 })),
            window,
            cx,
        );
        assert!(bad_args.is_err());
    });
}
