//! Birchpad's terminal without a window (ADR 0067), on `alacritty_terminal`: the [`Emulator`]
//! alone, a shell on a pseudo-terminal ([`Session`]), keys and pastes as bytes ([`keys`]), and
//! what the terminal shows with its colors resolved ([`Screen`]).

#[macro_use]
extern crate birchpad_i18n;

pub mod emulator;
pub mod keys;
pub mod screen;
pub mod session;

pub use emulator::{DEFAULT_SIZE, Emulator, clamp_size};
pub use keys::{Key, Modifiers};
pub use screen::{Cell, Palette, Rgb, Screen};
pub use session::{Exit, Session, SessionOptions, TerminalError, default_shell};
