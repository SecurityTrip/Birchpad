//! The emulator alone: bytes in, a screen out, without a process (tests, and what a session
//! runs on its thread).

use alacritty_terminal::Term;
use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::term::Config;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::vte::ansi::Processor;

use crate::screen::{Palette, Screen, screen_of};

/// The size a terminal starts with when it is given none.
pub const DEFAULT_SIZE: (u16, u16) = (80, 24);

/// The smallest terminal: what a panel too small to show one gets.
pub fn clamp_size(columns: u16, lines: u16) -> (u16, u16) {
    (columns.max(2), lines.max(1))
}

pub struct Emulator {
    term: Term<VoidListener>,
    processor: Processor,
}

impl Emulator {
    /// A terminal of `columns` by `lines` keeping `scrollback` lines above them.
    pub fn new(columns: u16, lines: u16, scrollback: usize) -> Self {
        let (columns, lines) = clamp_size(columns, lines);
        let config = Config {
            scrolling_history: scrollback,
            ..Config::default()
        };
        let size = TermSize::new(usize::from(columns), usize::from(lines));
        Self {
            term: Term::new(config, &size, VoidListener),
            processor: Processor::new(),
        }
    }

    /// What a program wrote.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.processor.advance(&mut self.term, bytes);
    }

    pub fn screen(&self, palette: &Palette) -> Screen {
        screen_of(&self.term, palette)
    }

    pub fn resize(&mut self, columns: u16, lines: u16) {
        let (columns, lines) = clamp_size(columns, lines);
        self.term
            .resize(TermSize::new(usize::from(columns), usize::from(lines)));
    }

    /// Scrolls back (positive) or forward (negative) through the scrollback.
    pub fn scroll(&mut self, lines: i32) {
        self.term.scroll_display(Scroll::Delta(lines));
    }

    pub fn scroll_to_bottom(&mut self) {
        self.term.scroll_display(Scroll::Bottom);
    }
}
