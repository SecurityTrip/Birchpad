//! Preferences > Terminal (ADR 0067): the shell of View > Terminal.

use serde::{Deserialize, Serialize};

/// The most lines the scrollback may keep.
pub const MAX_SCROLLBACK: u32 = 100_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct TerminalSettings {
    /// The shell's command line; empty for the system's (`%COMSPEC%` on Windows, `$SHELL`
    /// elsewhere).
    pub shell: String,
    /// The lines kept above the screen, at most [`MAX_SCROLLBACK`].
    pub scrollback: u32,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            shell: String::new(),
            scrollback: 10_000,
        }
    }
}

impl TerminalSettings {
    /// The scrollback, at most [`MAX_SCROLLBACK`] lines.
    pub fn scrollback_lines(&self) -> usize {
        self.scrollback.min(MAX_SCROLLBACK) as usize
    }
}
