//! Preferences > Language Servers (ADR 0065): the server of each language, none bundled.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct LspSettings {
    /// Whether servers run at all; off, none starts, configured or not.
    pub enabled: bool,
    /// The command line of each language's server, by Birchpad's language id
    /// (`rust = "rust-analyzer"`), split as a shell splits it.
    pub servers: BTreeMap<String, String>,
}

impl Default for LspSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            servers: BTreeMap::new(),
        }
    }
}

impl LspSettings {
    /// The command line of the server of language `id`: none while servers are off, or when
    /// the language has none or an empty one.
    pub fn server(&self, id: &str) -> Option<&str> {
        if !self.enabled {
            return None;
        }
        self.servers
            .get(id)
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
    }
}
