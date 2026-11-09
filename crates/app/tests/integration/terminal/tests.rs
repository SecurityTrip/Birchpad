//! View > Terminal (ADR 0067): the panel, its keys, and a shell in it.

use std::path::Path;
use std::time::{Duration, Instant};

use birchpad::app_state::AppState;
use birchpad::terminal::*;
use birchpad::workspace::Workspace;
use birchpad_commands::Invocation;
use birchpad_terminal::{Key, Modifiers};
use gpui_kit::{Bounds, Entity, Keystroke, TestAppContext, VisualTestContext, point, px, size};

use crate::workspace::open_workspace;

fn keystroke(text: &str, typed: Option<&str>) -> Keystroke {
    let mut keystroke = Keystroke::parse(text).unwrap();
    keystroke.key_char = typed.map(str::to_owned);
    keystroke
}

fn key(key: Key, shift: bool, alt: bool, control: bool) -> Option<Input> {
    Some(Input::Key(
        key,
        Modifiers {
            shift,
            alt,
            control,
        },
    ))
}

#[test]
fn keystrokes_become_what_the_shell_gets() {
    assert_eq!(
        input_of(&keystroke("a", Some("a"))),
        key(Key::Char('a'), false, false, false)
    );
    // Shift is in the character typed already.
    assert_eq!(
        input_of(&keystroke("shift-a", Some("A"))),
        key(Key::Char('A'), false, false, false)
    );
    // Another layout: the character typed, not the key.
    assert_eq!(
        input_of(&keystroke("f", Some("а"))),
        key(Key::Char('а'), false, false, false)
    );
    // Ctrl and Alt with a letter: the key.
    assert_eq!(
        input_of(&keystroke("ctrl-c", None)),
        key(Key::Char('c'), false, false, true)
    );
    assert_eq!(
        input_of(&keystroke("ctrl-r", Some("r"))),
        key(Key::Char('r'), false, false, true)
    );
    assert_eq!(
        input_of(&keystroke("alt-x", None)),
        key(Key::Char('x'), false, true, false)
    );
    for (text, named) in [
        ("enter", Key::Enter),
        ("tab", Key::Tab),
        ("backspace", Key::Backspace),
        ("escape", Key::Escape),
        ("up", Key::Up),
        ("pagedown", Key::PageDown),
        ("delete", Key::Delete),
        ("insert", Key::Insert),
        ("space", Key::Char(' ')),
        ("f1", Key::F(1)),
        ("f12", Key::F(12)),
    ] {
        assert_eq!(
            input_of(&keystroke(text, None)),
            key(named, false, false, false),
            "{text}"
        );
    }
    assert_eq!(
        input_of(&keystroke("shift-tab", None)),
        key(Key::Tab, true, false, false)
    );
    assert_eq!(
        input_of(&keystroke("ctrl-up", None)),
        key(Key::Up, false, false, true)
    );
    // Pasting.
    assert_eq!(
        input_of(&keystroke("ctrl-shift-v", None)),
        Some(Input::Paste)
    );
    assert_eq!(
        input_of(&keystroke("shift-insert", None)),
        Some(Input::Paste)
    );
}

#[test]
fn the_windows_own_keys_stay_its_own() {
    for text in [
        "ctrl-tab",
        "ctrl-shift-tab",
        "ctrl-pageup",
        "ctrl-pagedown",
        "ctrl-`",
    ] {
        assert_eq!(input_of(&keystroke(text, None)), None, "{text}");
    }
    let cmd = input_of(&keystroke("cmd-v", None));
    if cfg!(target_os = "macos") {
        assert_eq!(cmd, Some(Input::Paste));
    } else {
        assert_eq!(cmd, None);
    }
    assert_eq!(input_of(&keystroke("cmd-n", None)), None);
    // Keys a terminal has nothing for.
    assert_eq!(input_of(&keystroke("f13", None)), None);
    assert_eq!(input_of(&keystroke("f0", None)), None);
}

#[test]
fn the_grid_a_panel_has_room_for() {
    let bounds =
        |width: f32, height: f32| Bounds::new(point(px(10.), px(20.)), size(px(width), px(height)));
    assert_eq!(grid_of(bounds(800., 200.), px(8.), px(16.)), (100, 12));
    assert_eq!(grid_of(bounds(807.9, 207.9), px(8.), px(16.)), (100, 12));
    // Too small: the smallest terminal.
    assert_eq!(grid_of(bounds(1., 1.), px(8.), px(16.)), (2, 1));
    assert_eq!(grid_of(bounds(0., 0.), px(8.), px(16.)), (2, 1));
    // Huge: at most what a size can say.
    assert_eq!(
        grid_of(bounds(1e9, 1e9), px(1.), px(1.)),
        (u16::MAX, u16::MAX)
    );
}

#[test]
fn the_palette_follows_the_theme() {
    let palette = palette();
    let theme = birchpad::theme::current();
    assert_eq!(palette.background, theme.editor.background.to_rgb());
    assert_eq!(palette.foreground, theme.editor.text.to_rgb());
}

fn run(workspace: &Entity<Workspace>, invocation: Invocation, cx: &mut VisualTestContext) {
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.dispatch(&invocation, window, cx)
        })
        .unwrap();
    cx.run_until_parked();
}

fn wait_for(
    cx: &mut VisualTestContext,
    mut done: impl FnMut(&mut VisualTestContext) -> bool,
) -> bool {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        cx.run_until_parked();
        if done(cx) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    done(cx)
}

fn panel(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<TerminalPanel> {
    workspace.read_with(cx, |workspace, _| workspace.terminal.clone())
}

fn screen_text(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    let panel = panel(workspace, cx);
    panel.read_with(cx, |panel, _| {
        panel
            .screen()
            .map(|screen| screen.text())
            .unwrap_or_default()
    })
}

fn set_shell(shell: &str, cx: &mut VisualTestContext) {
    cx.update(|_, cx| cx.global_mut::<AppState>().settings.terminal.shell = shell.into());
}

/// The interactive shell of the platform.
fn shell() -> &'static str {
    if cfg!(windows) { "cmd.exe" } else { "/bin/sh" }
}

/// A shell that prints and exits.
fn printing(text: &str) -> String {
    if cfg!(windows) {
        format!("cmd.exe /c echo {text}")
    } else {
        format!("/bin/sh -c \"echo {text}\"")
    }
}

fn open_file_in(workspace: &Entity<Workspace>, folder: &Path, cx: &mut VisualTestContext) {
    let path = folder.join("notes.txt");
    std::fs::write(&path, "x").unwrap();
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(&path, window, cx)
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_terminal_runs_a_shell_in_the_documents_folder(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    open_file_in(&workspace, dir.path(), cx);
    set_shell(shell(), cx);
    run(&workspace, Invocation::new("view.terminal"), cx);
    let panel = panel(&workspace, cx);
    assert!(panel.read_with(cx, |panel, _| panel.visible));
    let started = panel.read_with(cx, |panel, _| panel.started.clone());
    assert_eq!(started.len(), 1);
    assert_eq!(
        std::fs::canonicalize(&started[0].folder).unwrap(),
        std::fs::canonicalize(dir.path()).unwrap()
    );
    assert_eq!(started[0].scrollback, 10_000);
    // Drawn: the session takes the size of the panel.
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("terminal").is_some());
    // Keys typed in the panel go to the shell, Ctrl+W too, which the window would close a tab
    // with.
    cx.simulate_keystrokes("e c h o space t y p e d");
    cx.simulate_keystrokes("enter");
    assert!(
        wait_for(cx, |cx| screen_text(&workspace, cx)
            .matches("typed")
            .count()
            >= 2),
        "{}",
        screen_text(&workspace, cx)
    );
    let tabs_before = workspace.read_with(cx, |workspace, cx| workspace.all_views(cx).len());
    cx.simulate_keystrokes("ctrl-w");
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.all_views(cx).len()),
        tabs_before,
        "the shell took Ctrl+W"
    );
    // Hidden and shown again: the same shell.
    run(&workspace, Invocation::new("view.terminal"), cx);
    assert!(!panel.read_with(cx, |panel, _| panel.visible));
    assert!(panel.read_with(cx, |panel, _| panel.is_running()));
    run(&workspace, Invocation::new("view.terminal"), cx);
    assert!(panel.read_with(cx, |panel, _| panel.visible));
    assert_eq!(panel.read_with(cx, |panel, _| panel.started.len()), 1);
}

#[gpui_kit::test]
fn a_shell_that_exits_is_started_again_with_enter(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.executor().allow_parking();
    set_shell(&printing("from-birchpad"), cx);
    run(&workspace, Invocation::new("view.terminal"), cx);
    let panel = panel(&workspace, cx);
    assert!(wait_for(cx, |cx| !panel.read_with(cx, |panel, _| panel.is_running())));
    assert!(screen_text(&workspace, cx).contains("from-birchpad"));
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("terminal-status").is_some(), "it says so");
    cx.simulate_keystrokes("enter");
    assert_eq!(panel.read_with(cx, |panel, _| panel.started.len()), 2);
    // Other keys do nothing while it is not running.
    assert!(wait_for(cx, |cx| !panel.read_with(cx, |panel, _| panel.is_running())));
    cx.simulate_keystrokes("x");
    assert_eq!(panel.read_with(cx, |panel, _| panel.started.len()), 2);
}

#[gpui_kit::test]
fn a_shell_that_cannot_start_says_why(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.executor().allow_parking();
    set_shell("\"unclosed", cx);
    run(&workspace, Invocation::new("view.terminal"), cx);
    let panel = panel(&workspace, cx);
    let error = panel.read_with(cx, |panel, _| panel.error.clone()).unwrap();
    assert!(error.contains("not closed"), "{error}");
    assert!(panel.read_with(cx, |panel, _| panel.session().is_none()));
    // A program that does not exist: refused, or started and gone at once.
    set_shell("birchpad-no-such-shell", cx);
    cx.simulate_keystrokes("enter");
    assert!(wait_for(cx, |cx| !panel.read_with(cx, |panel, _| panel.is_running())));
    let said = panel.read_with(cx, |panel, _| {
        panel.error.clone().or_else(|| {
            panel
                .session()
                .and_then(|session| session.exit())
                .map(|_| "exited".into())
        })
    });
    assert!(said.is_some());
    // Enter tries again.
    set_shell(shell(), cx);
    cx.simulate_keystrokes("enter");
    assert!(wait_for(cx, |cx| panel.read_with(cx, |panel, _| panel.is_running())));
}
