use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use birchpad_terminal::*;

fn options(program: &str, args: &[&str], folder: &std::path::Path) -> SessionOptions {
    SessionOptions {
        program: program.to_owned(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        folder: folder.to_owned(),
        // Wide enough for a prompt in a deep temporary folder and a command on one line.
        columns: 200,
        lines: 10,
        scrollback: 100,
    }
}

/// A shell command that prints `text` and exits.
fn echo(text: &str, folder: &std::path::Path) -> SessionOptions {
    if cfg!(windows) {
        options("cmd.exe", &["/c", &format!("echo {text}")], folder)
    } else {
        options("/bin/sh", &["-c", &format!("echo {text}")], folder)
    }
}

/// The interactive shell.
fn shell(folder: &std::path::Path) -> SessionOptions {
    if cfg!(windows) {
        options("cmd.exe", &[], folder)
    } else {
        options("/bin/sh", &[], folder)
    }
}

fn wait_for(done: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    done()
}

#[test]
fn a_shell_runs_prints_and_exits() {
    let dir = tempfile::tempdir().unwrap();
    let woken = Arc::new(AtomicUsize::new(0));
    let counted = woken.clone();
    let session = Session::spawn(&echo("birchpad-terminal", dir.path()), move || {
        counted.fetch_add(1, Ordering::SeqCst);
    })
    .unwrap();
    assert!(wait_for(|| session
        .screen(&Palette::DARK)
        .text()
        .contains("birchpad-terminal")));
    assert!(wait_for(|| session.exit().is_some()), "it exits");
    if let Some(code) = session.exit().unwrap().code {
        assert_eq!(code, 0);
    }
    assert!(woken.load(Ordering::SeqCst) > 0, "the window was told");
    assert_eq!(session.size(), (200, 10));
}

#[test]
fn what_is_typed_reaches_the_shell() {
    let dir = tempfile::tempdir().unwrap();
    let session = Session::spawn(&shell(dir.path()), || {}).unwrap();
    for c in "echo typed-in-birchpad".chars() {
        assert!(session.key(Key::Char(c), Modifiers::default()));
    }
    assert!(session.key(Key::Enter, Modifiers::default()));
    assert!(wait_for(|| session
        .screen(&Palette::DARK)
        .text()
        .matches("typed-in-birchpad")
        .count()
        >= 2));
    // A key that sends nothing.
    assert!(!session.key(Key::F(13), Modifiers::default()));
    // Pasted, then resized: the shell goes on.
    session.paste("echo pasted-in-birchpad\n");
    assert!(wait_for(|| session
        .screen(&Palette::LIGHT)
        .text()
        .matches("pasted-in-birchpad")
        .count()
        >= 2));
    session.resize(40, 8);
    assert_eq!(session.size(), (40, 8));
    let screen = session.screen(&Palette::LIGHT);
    assert_eq!((screen.columns, screen.lines), (40, 8));
    // Resizing to the same size changes nothing; too small, the smallest.
    session.resize(40, 8);
    session.resize(0, 0);
    assert_eq!(session.size(), (2, 1));
    session.scroll(5);
    session.scroll_to_bottom();
    assert_eq!(session.screen(&Palette::LIGHT).scrolled, 0);
    session.write(b"exit\r".to_vec());
    assert!(wait_for(|| session.exit().is_some()), "exit ends it");
    // Dropped after it exited: no panic.
    drop(session);
}

#[test]
fn a_shell_that_cannot_start_says_why() {
    let dir = tempfile::tempdir().unwrap();
    let missing = options("birchpad-no-such-shell", &[], dir.path());
    match Session::spawn(&missing, || {}) {
        Err(error) => assert!(
            error.to_string().contains("birchpad-no-such-shell"),
            "{error}"
        ),
        // Some systems start the process and see it fail: then it exits at once.
        Ok(session) => assert!(wait_for(|| session.exit().is_some())),
    }
    // A folder that is not one.
    let nowhere = shell(&dir.path().join("missing"));
    let error = Session::spawn(&nowhere, || {}).err().expect("refused");
    assert!(error.to_string().contains("missing"), "{error}");
}

#[test]
fn the_systems_shell() {
    let shell = default_shell();
    assert!(!shell.trim().is_empty());
    if cfg!(windows) {
        assert!(shell.to_ascii_lowercase().contains("cmd") || std::env::var("COMSPEC").is_ok());
    }
}
