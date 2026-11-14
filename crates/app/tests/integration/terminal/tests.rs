//! View > Terminal (ADR 0067): the panel, its keys, and a shell in it.

use std::path::Path;
use std::time::{Duration, Instant};

use birchpad::app_state::AppState;
use birchpad::terminal::*;
use birchpad::workspace::Workspace;
use birchpad_commands::Invocation;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use crate::workspace::open_workspace;

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
/// A shell that prints and exits a moment later: one that exits at once may be gone before the
/// pseudo-terminal has passed on what it printed.
fn printing(text: &str) -> String {
    if cfg!(windows) {
        format!("cmd.exe /c \"echo {text} & ping -n 2 127.0.0.1 >nul\"")
    } else {
        format!("/bin/sh -c \"echo {text}; sleep 1\"")
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
    // The exit can be seen before the last of the output is read.
    assert!(wait_for(cx, |cx| screen_text(&workspace, cx)
        .contains("from-birchpad")));
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
