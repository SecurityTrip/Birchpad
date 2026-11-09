//! View > Terminal (ADR 0067): the user's shell in a panel under the documents, one session per
//! window, started the first time the panel shows, in the active document's folder.
//!
//! The panel draws the session's screen cell by cell in the editor's font, with the theme's
//! text and background colors and the 16 colors of ANSI of its brightness. While it has the
//! focus, it takes the keys before the window's shortcuts do, but for those that switch tabs,
//! hide it, or hold Cmd; Ctrl+Shift+V and Shift+Insert paste.

use std::cell::Cell as Shared;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{Result, anyhow};
use birchpad_terminal::{Key, Modifiers, Palette, Screen, Session, SessionOptions};
use futures::StreamExt as _;
use gpui_kit::{
    App, Bounds, Context, FocusHandle, Focusable, Font, Keystroke, MouseButton, Pixels,
    ScrollWheelEvent, Subscription, Task, TextRun, Window, canvas, div, fill, outline, point,
    prelude::*, px, size,
};

use crate::app_state::AppState;
use crate::commands::{CommandRegistry, Register as _};
use crate::workspace::Workspace;

/// The key context of the panel, which takes the keys.
pub const CONTEXT: &str = "Terminal";

/// What a keystroke sends to the shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Key(Key, Modifiers),
    /// Paste the clipboard.
    Paste,
}

/// What the panel takes of a keystroke: none of those that switch tabs (`Ctrl+Tab`,
/// `Ctrl+PageUp`, `Ctrl+PageDown`), hide the panel (``Ctrl+` ``) or hold Cmd or the Windows key,
/// which stay the window's.
pub fn input_of(keystroke: &Keystroke) -> Option<Input> {
    let held = &keystroke.modifiers;
    let key = keystroke.key.as_str();
    if held.platform || held.function {
        // Cmd+V pastes on macOS, as everywhere there.
        return (cfg!(target_os = "macos") && held.platform && key == "v").then_some(Input::Paste);
    }
    if held.control && matches!(key, "tab" | "pageup" | "pagedown" | "`") {
        return None;
    }
    if (held.control && held.shift && key == "v") || (held.shift && key == "insert") {
        return Some(Input::Paste);
    }
    let modifiers = Modifiers {
        shift: held.shift,
        alt: held.alt,
        control: held.control,
    };
    let named = match key {
        "enter" => Some(Key::Enter),
        "tab" => Some(Key::Tab),
        "backspace" => Some(Key::Backspace),
        "escape" => Some(Key::Escape),
        "up" => Some(Key::Up),
        "down" => Some(Key::Down),
        "left" => Some(Key::Left),
        "right" => Some(Key::Right),
        "home" => Some(Key::Home),
        "end" => Some(Key::End),
        "pageup" => Some(Key::PageUp),
        "pagedown" => Some(Key::PageDown),
        "insert" => Some(Key::Insert),
        "delete" => Some(Key::Delete),
        "space" => Some(Key::Char(' ')),
        _ => key
            .strip_prefix('f')
            .and_then(|number| number.parse::<u8>().ok())
            .filter(|number| (1..=12).contains(number))
            .map(Key::F),
    };
    if let Some(named) = named {
        return Some(Input::Key(named, modifiers));
    }
    // A character: as typed, Shift and the layout applied, or the key itself with Ctrl or Alt.
    let typed = keystroke
        .key_char
        .as_deref()
        .filter(|typed| !held.control && !typed.is_empty())
        .unwrap_or(key);
    let mut chars = typed.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => Some(Input::Key(
            Key::Char(c),
            Modifiers {
                // Shift is in the character already.
                shift: held.shift && held.control,
                ..modifiers
            },
        )),
        _ => None,
    }
}

/// The colors of the terminal for the theme in use: its text and background, the cursor in its
/// caret's color, and the 16 of ANSI of its brightness.
pub fn palette() -> Palette {
    let theme = crate::theme::current();
    let base = if theme.dark {
        Palette::DARK
    } else {
        Palette::LIGHT
    };
    Palette {
        foreground: theme.editor.text.to_rgb(),
        background: theme.editor.background.to_rgb(),
        cursor: theme.editor.caret.to_rgb(),
        ..base
    }
}

/// The size of a cell and of a line in the editor's font.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Metrics {
    cell: Pixels,
    line_height: Pixels,
    font_size: Pixels,
}

fn metrics(font: &Font, font_size: Pixels, window: &mut Window) -> Metrics {
    let text_system = window.text_system();
    let font_id = text_system.resolve_font(font);
    let cell = text_system
        .advance(font_id, font_size, 'm')
        .map(|advance| advance.width)
        .unwrap_or(font_size * 0.6);
    Metrics {
        cell: cell.max(px(1.)),
        line_height: (font_size * 1.3).round().max(px(1.)),
        font_size,
    }
}

/// How many columns and lines `bounds` holds.
pub fn grid_of(bounds: Bounds<Pixels>, cell: Pixels, line_height: Pixels) -> (u16, u16) {
    let columns = (bounds.size.width / cell).floor().max(0.) as u32;
    let lines = (bounds.size.height / line_height).floor().max(0.) as u32;
    birchpad_terminal::clamp_size(
        columns.min(u32::from(u16::MAX)) as u16,
        lines.min(u32::from(u16::MAX)) as u16,
    )
}

pub struct TerminalPanel {
    focus_handle: FocusHandle,
    /// Whether View > Terminal shows it.
    pub visible: bool,
    session: Option<Session>,
    /// Why the last shell could not start.
    pub error: Option<String>,
    /// The folder the last shell started in, where Enter starts the next.
    folder: Option<PathBuf>,
    /// The grid the last frame had room for.
    grid: Rc<Shared<Option<(u16, u16)>>>,
    /// The sessions started, for tests to read.
    #[cfg(feature = "test-support")]
    pub started: Vec<SessionOptions>,
    _waking: Option<Task<()>>,
    _keys: Subscription,
}

impl Focusable for TerminalPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl TerminalPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let this = cx.entity().downgrade();
        // Before the window's shortcuts: the shell's keys (Ctrl+R, Ctrl+W...) are its own.
        let keys = cx.intercept_keystrokes(move |event, window, cx| {
            let in_terminal = event
                .context_stack
                .iter()
                .any(|context| context.contains(CONTEXT));
            if !in_terminal {
                return;
            }
            let Some(input) = input_of(&event.keystroke) else {
                return;
            };
            let took = this
                .update(cx, |this, cx| this.take(input, window, cx))
                .unwrap_or(false);
            if took {
                cx.stop_propagation();
            }
        });
        Self {
            focus_handle: cx.focus_handle(),
            visible: false,
            session: None,
            error: None,
            folder: None,
            grid: Rc::default(),
            #[cfg(feature = "test-support")]
            started: Vec::new(),
            _waking: None,
            _keys: keys,
        }
    }

    /// The running session, if one started.
    pub fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    /// Whether a shell runs (started, not exited).
    pub fn is_running(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.exit().is_none())
    }

    /// The screen of the session, in the theme's colors.
    pub fn screen(&self) -> Option<Screen> {
        self.session
            .as_ref()
            .map(|session| session.screen(&palette()))
    }

    /// The shell's command line: `terminal.shell`, or the system's shell.
    fn shell(cx: &App) -> Result<(String, Vec<String>)> {
        let setting = AppState::global(cx)
            .settings
            .terminal
            .shell
            .trim()
            .to_owned();
        if setting.is_empty() {
            return Ok((birchpad_terminal::default_shell(), Vec::new()));
        }
        let line =
            birchpad_lsp::CommandLine::parse(&setting).map_err(|error| anyhow!("{error}"))?;
        Ok((line.program, line.args))
    }

    /// Starts a shell in `folder`; what went wrong is kept to show.
    pub fn start(&mut self, folder: PathBuf, cx: &mut Context<Self>) -> Result<()> {
        self.session = None;
        self.error = None;
        self.folder = Some(folder.clone());
        let (columns, lines) = self.grid.get().unwrap_or(birchpad_terminal::DEFAULT_SIZE);
        let result = Self::shell(cx).and_then(|(program, args)| {
            let options = SessionOptions {
                program,
                args,
                folder,
                columns,
                lines,
                scrollback: AppState::global(cx).settings.terminal.scrollback_lines(),
            };
            #[cfg(feature = "test-support")]
            self.started.push(options.clone());
            let (wake, mut woken) = futures::channel::mpsc::unbounded::<()>();
            let session = Session::spawn(&options, move || {
                let _ = wake.unbounded_send(());
            })
            .map_err(|error| anyhow!("{error}"))?;
            self._waking = Some(cx.spawn(async move |this, cx| {
                while woken.next().await.is_some() {
                    // A burst of output is drawn once.
                    while woken.try_recv().is_ok() {}
                    if this.update(cx, |_, cx| cx.notify()).is_err() {
                        break;
                    }
                }
            }));
            Ok(session)
        });
        match result {
            Ok(session) => {
                self.session = Some(session);
                cx.notify();
                Ok(())
            }
            Err(error) => {
                self.error = Some(format!("{error:#}"));
                cx.notify();
                Err(error)
            }
        }
    }

    /// What the keys do: sent to the shell, or, once it exited, Enter starts another.
    fn take(&mut self, input: Input, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let running = self.is_running();
        match input {
            Input::Key(Key::Enter, _) if !running => {
                if let Some(folder) = self.folder.clone() {
                    let _ = self.start(folder, cx);
                }
                true
            }
            _ if !running => false,
            Input::Paste => {
                let text = cx
                    .read_from_clipboard()
                    .and_then(|item| item.text())
                    .unwrap_or_default();
                if let Some(session) = &self.session
                    && !text.is_empty()
                {
                    session.paste(&text);
                }
                true
            }
            Input::Key(key, modifiers) => {
                let _ = window;
                self.session
                    .as_ref()
                    .is_some_and(|session| session.key(key, modifiers))
            }
        }
    }

    fn scroll(&mut self, event: &ScrollWheelEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = &self.session else {
            return;
        };
        let line_height = window.line_height();
        let lines = (event.delta.pixel_delta(line_height).y / line_height).round() as i32;
        if lines != 0 {
            session.scroll(lines);
            cx.notify();
        }
    }

    /// The line under the screen: why the shell did not start, or that it exited.
    fn status(&self) -> Option<String> {
        if let Some(error) = &self.error {
            return Some(error.clone());
        }
        let exit = self.session.as_ref()?.exit()?;
        Some(match exit.code {
            Some(code) => t!("terminal-panel.exited-with", code = code),
            None => t!("terminal-panel.exited"),
        })
    }
}

/// Paints `screen` in `bounds`: each run of cells alike with its background, then its text.
fn paint_screen(
    screen: &Screen,
    bounds: Bounds<Pixels>,
    metrics: Metrics,
    font: &Font,
    focused: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let rgb = |color: u32| -> gpui_kit::Hsla { gpui_kit::rgb(color).into() };
    for (line, row) in screen.rows.iter().enumerate() {
        let y = bounds.top() + metrics.line_height * line as f32;
        let mut column = 0;
        while column < row.len() {
            let first = row[column];
            let mut end = column + 1;
            while end < row.len() {
                let next = row[end];
                let alike = next.foreground == first.foreground
                    && next.background == first.background
                    && next.bold == first.bold
                    && next.italic == first.italic
                    && next.underline == first.underline
                    && !next.wide
                    && !first.wide;
                if !alike {
                    break;
                }
                end += 1;
            }
            let x = bounds.left() + metrics.cell * column as f32;
            let width = metrics.cell * (end - column) as f32;
            let cells = Bounds::new(point(x, y), size(width, metrics.line_height));
            if first.background != screen_background(screen) {
                window.paint_quad(fill(cells, rgb(first.background)));
            }
            let text: String = row[column..end].iter().map(|cell| cell.character).collect();
            if !text.trim().is_empty() {
                let mut run_font = font.clone();
                if first.bold {
                    run_font.weight = gpui_kit::FontWeight::BOLD;
                }
                if first.italic {
                    run_font.style = gpui_kit::FontStyle::Italic;
                }
                let run = TextRun {
                    len: text.len(),
                    font: run_font,
                    color: rgb(first.foreground),
                    background_color: None,
                    underline: first.underline.then(|| gpui_kit::UnderlineStyle {
                        color: Some(rgb(first.foreground)),
                        thickness: px(1.),
                        wavy: false,
                    }),
                    strikethrough: None,
                };
                let shaped =
                    window
                        .text_system()
                        .shape_line(text.into(), metrics.font_size, &[run], None);
                let _ = shaped.paint(
                    point(x, y),
                    metrics.line_height,
                    gpui_kit::TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }
            column = end;
        }
    }
    if let Some((line, column)) = screen.cursor {
        let cursor = Bounds::new(
            point(
                bounds.left() + metrics.cell * column as f32,
                bounds.top() + metrics.line_height * line as f32,
            ),
            size(metrics.cell, metrics.line_height),
        );
        let color = rgb(palette().cursor);
        if focused {
            window.paint_quad(fill(cursor, color.opacity(0.6)));
        } else {
            window.paint_quad(outline(cursor, color, gpui_kit::BorderStyle::Solid));
        }
    }
}

/// The background most of the screen has: the palette's, not painted cell by cell.
fn screen_background(_: &Screen) -> u32 {
    palette().background
}

impl Render for TerminalPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = crate::editor::ViewSettings::read(cx);
        let font = gpui_kit::font(settings.font_family.clone());
        let metrics = metrics(&font, settings.font_size, window);
        let screen = self.screen();
        let focused = self.focus_handle.is_focused(window);
        let grid = self.grid.clone();
        let entity = cx.entity().downgrade();
        let background = gpui_kit::rgb(palette().background);
        let status = self.status();
        div()
            .id("terminal")
            .debug_selector(|| "terminal".into())
            .track_focus(&self.focus_handle)
            .key_context(CONTEXT)
            .size_full()
            .flex()
            .flex_col()
            .bg(background)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    window.focus(&this.focus_handle, cx);
                }),
            )
            .on_scroll_wheel(cx.listener(Self::scroll))
            .child(
                div().flex_1().min_h(px(0.)).p_1().child(
                    canvas(
                        move |bounds, _, cx| {
                            let wanted = grid_of(bounds, metrics.cell, metrics.line_height);
                            if grid.get() != Some(wanted) {
                                grid.set(Some(wanted));
                                // The session takes the size the panel has room for.
                                entity
                                    .update(cx, |this, cx| {
                                        if let Some(session) = &this.session {
                                            session.resize(wanted.0, wanted.1);
                                        }
                                        cx.notify();
                                    })
                                    .ok();
                            }
                        },
                        move |bounds, (), window, cx| {
                            if let Some(screen) = &screen {
                                paint_screen(screen, bounds, metrics, &font, focused, window, cx);
                            }
                        },
                    )
                    .size_full(),
                ),
            )
            .children(status.map(|status| {
                div()
                    .debug_selector(|| "terminal-status".into())
                    .px_2()
                    .py_1()
                    .text_sm()
                    .text_color(crate::theme::paint(crate::theme::ui().muted))
                    .bg(crate::theme::paint(crate::theme::ui().surface))
                    .child(format!("{status} {}", t!("terminal-panel.press-enter")))
            }))
    }
}

/// The folder a new shell starts in: the active document's, else the home folder.
fn start_folder(workspace: &Workspace, cx: &App) -> PathBuf {
    workspace
        .active_view(cx)
        .and_then(|view| {
            view.read(cx)
                .buffer
                .read(cx)
                .path()
                .and_then(|path| path.parent())
                .map(std::path::Path::to_owned)
        })
        .filter(|folder| folder.is_dir())
        .or_else(std::env::home_dir)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

impl Workspace {
    /// View > Terminal: shows the panel (starting a shell the first time) and gives it the
    /// focus, or hides it and gives the focus back to the document.
    pub fn toggle_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let terminal = self.terminal.clone();
        if terminal.read(cx).visible {
            terminal.update(cx, |terminal, cx| {
                terminal.visible = false;
                cx.notify();
            });
            if let Some(view) = self.active_view(cx) {
                let focus = view.read(cx).focus_handle.clone();
                window.focus(&focus, cx);
            }
        } else {
            let folder = start_folder(self, cx);
            terminal.update(cx, |terminal, cx| {
                terminal.visible = true;
                if terminal.session.is_none() {
                    // What went wrong shows in the panel.
                    let _ = terminal.start(folder, cx);
                }
                cx.notify();
            });
            let focus = terminal.read(cx).focus_handle.clone();
            window.focus(&focus, cx);
        }
        self.refresh_menus(cx);
        cx.notify();
    }
}

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("view.terminal", |this, (), window, cx| {
        this.toggle_terminal(window, cx);
        Ok(())
    });
}
