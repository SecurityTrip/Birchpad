//! A shell on a pseudo-terminal: Alacritty's event loop reads its output into the emulator on a
//! thread of its own and writes what it is sent; the session answers the program's questions
//! (its size, its colors) and says when it exits.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use alacritty_terminal::Term;
use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Config;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::tty;

use crate::emulator::clamp_size;
use crate::keys::{self, Key, Modifiers};
use crate::screen::{Palette, Screen, screen_of};

/// The pixel size of a cell the program is told, when it asks; panels do not say theirs.
const CELL: (u16, u16) = (8, 16);

/// Why a shell could not start.
#[derive(Debug, thiserror::Error)]
pub enum TerminalError {
    #[error("{}", t!("terminal.cannot-start", program = .program, reason = .reason))]
    CannotStart { program: String, reason: String },
}

/// What starts a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionOptions {
    pub program: String,
    pub args: Vec<String>,
    /// The folder the shell starts in.
    pub folder: PathBuf,
    pub columns: u16,
    pub lines: u16,
    pub scrollback: usize,
}

/// How the shell ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exit {
    /// Its exit code, when the system says.
    pub code: Option<i32>,
}

#[derive(Default)]
struct Shared {
    title: Option<String>,
    exit: Option<Exit>,
    sender: Option<EventLoopSender>,
    size: Option<WindowSize>,
    palette: Option<Palette>,
}

type Wake = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone)]
struct Listener {
    shared: Arc<Mutex<Shared>>,
    wake: Wake,
}

impl Listener {
    fn write(&self, text: String) {
        let shared = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(sender) = &shared.sender {
            let _ = sender.send(Msg::Input(Cow::Owned(text.into_bytes())));
        }
    }
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        match event {
            Event::Title(title) => {
                self.shared.lock().unwrap_or_else(|e| e.into_inner()).title = Some(title);
            }
            Event::ResetTitle => {
                self.shared.lock().unwrap_or_else(|e| e.into_inner()).title = None;
            }
            Event::PtyWrite(text) => self.write(text),
            Event::TextAreaSizeRequest(format) => {
                let size = self.shared.lock().unwrap_or_else(|e| e.into_inner()).size;
                if let Some(size) = size {
                    self.write(format(size));
                }
            }
            Event::ColorRequest(index, format) => {
                let palette = self
                    .shared
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .palette
                    .unwrap_or(Palette::DARK);
                let color = match index {
                    0..=255 => palette.indexed(index as u8),
                    257 => palette.background,
                    258 => palette.cursor,
                    _ => palette.foreground,
                };
                let rgb = alacritty_terminal::vte::ansi::Rgb {
                    r: (color >> 16) as u8,
                    g: (color >> 8) as u8,
                    b: color as u8,
                };
                self.write(format(rgb));
            }
            Event::ChildExit(status) => {
                self.shared.lock().unwrap_or_else(|e| e.into_inner()).exit = Some(Exit {
                    code: status.code(),
                });
            }
            Event::Exit => {
                let mut shared = self.shared.lock().unwrap_or_else(|e| e.into_inner());
                shared.exit.get_or_insert(Exit { code: None });
            }
            // The clipboard is the user's to give: OSC 52 is not answered.
            Event::ClipboardStore(..) | Event::ClipboardLoad(..) => {}
            Event::Wakeup | Event::Bell | Event::MouseCursorDirty | Event::CursorBlinkingChange => {
            }
        }
        (self.wake)();
    }
}

/// A running shell and its screen.
pub struct Session {
    term: Arc<FairMutex<Term<Listener>>>,
    shared: Arc<Mutex<Shared>>,
    sender: EventLoopSender,
}

fn window_size(columns: u16, lines: u16) -> WindowSize {
    WindowSize {
        num_lines: lines,
        num_cols: columns,
        cell_width: CELL.0,
        cell_height: CELL.1,
    }
}

impl Session {
    /// Starts `options.program` on a pseudo-terminal; `wake` is called, from another thread,
    /// whenever the screen may have changed or the shell exited.
    pub fn spawn(
        options: &SessionOptions,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, TerminalError> {
        let (columns, lines) = clamp_size(options.columns, options.lines);
        let cannot_start = |reason: String| TerminalError::CannotStart {
            program: options.program.clone(),
            reason,
        };
        let env = HashMap::from([
            ("TERM".to_owned(), "xterm-256color".to_owned()),
            ("COLORTERM".to_owned(), "truecolor".to_owned()),
        ]);
        let pty_options = tty::Options {
            shell: Some(tty::Shell::new(
                options.program.clone(),
                options.args.clone(),
            )),
            working_directory: Some(options.folder.clone()),
            drain_on_exit: true,
            env,
            #[cfg(windows)]
            escape_args: true,
        };
        if !options.folder.is_dir() {
            return Err(cannot_start(format!(
                "{} is not a folder",
                options.folder.display()
            )));
        }
        let size = window_size(columns, lines);
        let pty =
            tty::new(&pty_options, size, 0).map_err(|error| cannot_start(error.to_string()))?;
        let shared = Arc::new(Mutex::new(Shared {
            size: Some(size),
            ..Shared::default()
        }));
        let listener = Listener {
            shared: shared.clone(),
            wake: Arc::new(wake),
        };
        let config = Config {
            scrolling_history: options.scrollback,
            ..Config::default()
        };
        let term = Arc::new(FairMutex::new(Term::new(
            config,
            &TermSize::new(usize::from(columns), usize::from(lines)),
            listener.clone(),
        )));
        let event_loop = EventLoop::new(term.clone(), listener, pty, true, false)
            .map_err(|error| cannot_start(error.to_string()))?;
        let sender = event_loop.channel();
        shared.lock().unwrap_or_else(|e| e.into_inner()).sender = Some(sender.clone());
        event_loop.spawn();
        Ok(Self {
            term,
            shared,
            sender,
        })
    }

    /// The screen now, its colors through `palette`.
    pub fn screen(&self, palette: &Palette) -> Screen {
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .palette = Some(*palette);
        screen_of(&self.term.lock(), palette)
    }

    /// Sends bytes to the shell, as typed.
    pub fn write(&self, bytes: Vec<u8>) {
        let _ = self.sender.send(Msg::Input(Cow::Owned(bytes)));
    }

    /// Sends a key; false for one that sends nothing. The view goes back to the bottom.
    pub fn key(&self, key: Key, modifiers: Modifiers) -> bool {
        let app_cursor = self
            .term
            .lock()
            .mode()
            .contains(alacritty_terminal::term::TermMode::APP_CURSOR);
        let Some(bytes) = keys::encode(key, modifiers, app_cursor) else {
            return false;
        };
        self.scroll_to_bottom();
        self.write(bytes);
        true
    }

    /// Pastes `text`, bracketed when the program asked.
    pub fn paste(&self, text: &str) {
        let bracketed = self
            .term
            .lock()
            .mode()
            .contains(alacritty_terminal::term::TermMode::BRACKETED_PASTE);
        self.scroll_to_bottom();
        self.write(keys::paste(text, bracketed));
    }

    /// The terminal takes `columns` by `lines`; the program is told.
    pub fn resize(&self, columns: u16, lines: u16) {
        let (columns, lines) = clamp_size(columns, lines);
        let size = window_size(columns, lines);
        {
            let mut shared = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            if shared
                .size
                .is_some_and(|known| (known.num_cols, known.num_lines) == (columns, lines))
            {
                return;
            }
            shared.size = Some(size);
        }
        self.term
            .lock()
            .resize(TermSize::new(usize::from(columns), usize::from(lines)));
        let _ = self.sender.send(Msg::Resize(size));
    }

    /// The size the terminal has.
    pub fn size(&self) -> (u16, u16) {
        let shared = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        shared.size.map_or(crate::emulator::DEFAULT_SIZE, |size| {
            (size.num_cols, size.num_lines)
        })
    }

    /// Scrolls back (positive) or forward (negative).
    pub fn scroll(&self, lines: i32) {
        self.term.lock().scroll_display(Scroll::Delta(lines));
    }

    pub fn scroll_to_bottom(&self) {
        self.term.lock().scroll_display(Scroll::Bottom);
    }

    /// The title the program set.
    pub fn title(&self) -> Option<String> {
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .title
            .clone()
    }

    /// How the shell ended, once it has.
    pub fn exit(&self) -> Option<Exit> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner()).exit
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.sender.send(Msg::Shutdown);
    }
}

/// The system's shell: `%COMSPEC%` on Windows (`cmd.exe` without it), `$SHELL` elsewhere
/// (`/bin/sh` without it).
pub fn default_shell() -> String {
    let (variable, fallback) = if cfg!(windows) {
        ("COMSPEC", "cmd.exe")
    } else {
        ("SHELL", "/bin/sh")
    };
    std::env::var(variable)
        .ok()
        .map(|shell| shell.trim().to_owned())
        .filter(|shell| !shell.is_empty())
        .unwrap_or_else(|| fallback.to_owned())
}
