//! Merging configuration layers into effective settings.

use std::collections::BTreeSet;
use std::fmt;

use toml::{Table, Value};

use crate::settings::Settings;

/// Where a piece of configuration comes from, from lowest to highest priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Layer {
    /// Built into the application.
    Defaults,
    /// Defaults an administrator set for every user of the machine.
    Machine,
    /// The user's own settings.
    User,
    /// Administrator policies. They win over everything and the user cannot change them.
    Policy,
}

impl fmt::Display for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Defaults => "built-in defaults",
            Self::Machine => "machine defaults",
            Self::User => "user settings",
            Self::Policy => "administrator policy",
        })
    }
}

/// A problem found while loading configuration. The offending file or key is skipped and
/// everything else still applies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub layer: Layer,
    /// Dotted key, if the problem is about a single key.
    pub key: Option<String>,
    pub message: String,
}

/// The raw contents of each configuration layer, as parsed from files or the registry.
#[derive(Debug, Clone, Default)]
pub struct Sources {
    pub machine: Option<Table>,
    pub user: Option<Table>,
    pub policy: Option<Table>,
    /// Problems found while reading the sources, carried over into the result.
    pub diagnostics: Vec<Diagnostic>,
}

/// Effective settings with a record of which keys administrators locked.
#[derive(Debug, Clone)]
pub struct ResolvedSettings {
    pub settings: Settings,
    pub diagnostics: Vec<Diagnostic>,
    locked: BTreeSet<String>,
}

impl ResolvedSettings {
    /// True if `key` (dotted, e.g. `updates.mode`) is set by policy and must be shown read-only.
    pub fn is_locked(&self, key: &str) -> bool {
        self.locked.contains(key)
    }

    pub fn locked_keys(&self) -> impl Iterator<Item = &str> {
        self.locked.iter().map(String::as_str)
    }

    /// The keys set by policy with their values in TOML syntax, e.g. `("updates.mode", "\"off\"")`.
    pub fn policies(&self) -> Vec<(String, String)> {
        let table = Table::try_from(&self.settings).expect("settings serialize to a table");
        self.locked
            .iter()
            .filter_map(|key| {
                let mut parts = key.split('.');
                let first = table.get(parts.next()?)?;
                let value = parts.try_fold(first, |value, part| value.get(part))?;
                Some((key.clone(), value.to_string()))
            })
            .collect()
    }
}

/// Merges the layers in priority order.
///
/// Keys are applied one at a time, so a single invalid value (say, `tab-width = "four"`) is
/// reported and skipped instead of discarding the whole file.
pub fn resolve(sources: Sources) -> ResolvedSettings {
    let mut merged = Table::try_from(Settings::default()).expect("defaults serialize to a table");
    let mut diagnostics = sources.diagnostics;
    let mut locked = BTreeSet::new();

    let layers = [
        (Layer::Machine, sources.machine),
        (Layer::User, sources.user),
        (Layer::Policy, sources.policy),
    ];
    for (layer, table) in layers {
        let Some(table) = table else { continue };
        for (key, value) in leaves(table) {
            let mut candidate = merged.clone();
            set_path(&mut candidate, &key, value);
            match candidate.clone().try_into::<Settings>() {
                Ok(_) => {
                    merged = candidate;
                    if layer == Layer::Policy {
                        locked.insert(key);
                    }
                }
                Err(error) => diagnostics.push(Diagnostic {
                    layer,
                    message: error.message().to_owned(),
                    key: Some(key),
                }),
            }
        }
    }

    let mut settings: Settings = merged.try_into().expect("every merged key was validated");
    // A language with nothing of its own (only unknown keys under it) is no language entry: it
    // would not survive being written out and read again.
    settings
        .languages
        .retain(|_, own| own.tab_width.is_some() || own.insert_spaces.is_some());
    ResolvedSettings {
        settings,
        diagnostics,
        locked,
    }
}

/// Sets a dotted `key` in `table`, creating intermediate tables.
pub(crate) fn set_path(table: &mut Table, key: &str, value: Value) {
    let mut parts = key.split('.').peekable();
    let mut current = table;
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            current.insert(part.to_owned(), value);
            return;
        }
        let entry = current
            .entry(part)
            .or_insert_with(|| Value::Table(Table::new()));
        if !entry.is_table() {
            *entry = Value::Table(Table::new());
        }
        current = entry.as_table_mut().expect("just ensured a table");
    }
}

/// Flattens nested tables into `(dotted key, value)` pairs. Arrays are values, not tables.
fn leaves(table: Table) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    let mut stack = vec![(String::new(), table)];
    while let Some((prefix, table)) = stack.pop() {
        for (key, value) in table {
            let path = if prefix.is_empty() {
                key
            } else {
                format!("{prefix}.{key}")
            };
            match value {
                Value::Table(nested) => stack.push((path, nested)),
                value => out.push((path, value)),
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[cfg(test)]
#[path = "../tests/unit/layers/tests.rs"]
mod tests;
