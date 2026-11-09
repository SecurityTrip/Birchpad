//! Preferences > Git (ADR 0066): the git program Birchpad asks.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct GitSettings {
    /// Whether Birchpad asks Git at all: the margin, blame and Compare to Git HEAD.
    pub enabled: bool,
    /// The program to run: `git` on the `PATH`, or a path to it.
    pub program: String,
}

impl Default for GitSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            program: "git".to_owned(),
        }
    }
}

impl GitSettings {
    /// The program to run, `git` when the setting is blank.
    pub fn program(&self) -> &str {
        match self.program.trim() {
            "" => "git",
            program => program,
        }
    }
}
