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

    let settings = merged.try_into().expect("every merged key was validated");
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
mod tests {
    use super::*;
    use crate::settings::{UpdateChannel, UpdateMode};

    fn table(toml: &str) -> Option<Table> {
        Some(toml::from_str(toml).unwrap())
    }

    #[test]
    fn later_layers_win_and_policy_locks() {
        let resolved = resolve(Sources {
            machine: table("updates.mode = 'notify'\neditor.tab-width = 2"),
            user: table("updates.mode = 'auto'\nupdates.channel = 'beta'\neditor.tab-width = 8"),
            policy: table("[updates]\nmode = 'off'"),
            diagnostics: Vec::new(),
        });

        assert_eq!(resolved.settings.updates.mode, UpdateMode::Off);
        assert_eq!(resolved.settings.updates.channel, UpdateChannel::Beta);
        assert_eq!(resolved.settings.editor.tab_width, 8);
        assert!(resolved.is_locked("updates.mode"));
        assert!(!resolved.is_locked("updates.channel"));
        assert!(resolved.diagnostics.is_empty());
        assert_eq!(
            resolved.policies(),
            [("updates.mode".to_owned(), "\"off\"".to_owned())]
        );
    }

    #[test]
    fn invalid_key_is_skipped_others_apply() {
        let resolved = resolve(Sources {
            user: table("editor.tab-width = 'four'\neditor.word-wrap = true\nunknown.key = 1"),
            ..Sources::default()
        });

        assert_eq!(resolved.settings.editor.tab_width, 4);
        assert!(resolved.settings.editor.word_wrap);
        assert_eq!(resolved.diagnostics.len(), 1);
        assert_eq!(resolved.diagnostics[0].layer, Layer::User);
        assert_eq!(
            resolved.diagnostics[0].key.as_deref(),
            Some("editor.tab-width")
        );
    }

    #[test]
    fn invalid_policy_value_does_not_lock() {
        let resolved = resolve(Sources {
            policy: table("updates.mode = 'sometimes'"),
            ..Sources::default()
        });
        assert_eq!(resolved.settings.updates.mode, UpdateMode::Auto);
        assert!(!resolved.is_locked("updates.mode"));
        assert_eq!(resolved.diagnostics.len(), 1);
    }

    #[test]
    fn highlighting_settings_nest_two_levels() {
        let resolved = resolve(Sources {
            user: table(
                "highlighting.smart.enabled = false\nhighlighting.token-style.match-case = true",
            ),
            ..Sources::default()
        });
        let highlighting = &resolved.settings.highlighting;
        assert!(!highlighting.smart.enabled);
        assert!(highlighting.smart.whole_word, "defaults stay");
        assert!(highlighting.token_style.match_case);
        assert!(resolved.diagnostics.is_empty());
    }

    #[test]
    fn edge_and_current_line_settings() {
        use crate::settings::{CurrentLine, Edge};
        let resolved = resolve(Sources {
            user: table(
                "editor.edge = 'line'\neditor.edge-columns = [80, 120]\n\
                 editor.current-line = 'frame'\neditor.edge-column = 72",
            ),
            ..Sources::default()
        });
        let editor = &resolved.settings.editor;
        assert_eq!(editor.edge, Edge::Line);
        assert_eq!(editor.edge_columns, [80, 120]);
        assert_eq!(editor.current_line, CurrentLine::Frame);
        assert!(editor.indent_guides, "on by default, as in Notepad++");
        assert!(!editor.show_whitespace && !editor.show_eol && !editor.wrap_symbol);
        // The edge-column of earlier versions is ignored like any unknown key.
        assert!(resolved.diagnostics.is_empty());
    }
}
