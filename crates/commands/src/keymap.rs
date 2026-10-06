//! Keymaps: which keystrokes invoke which commands, as data.
//!
//! A keymap is built from layers, like settings: the built-in defaults, then the user's keymap
//! file. A later binding of the same keys in the same context replaces an earlier one; a user
//! binding with `unbind = true` removes it. Problems are collected as diagnostics instead of
//! failing, so one bad line never costs the user all their shortcuts.

use std::fmt;

use serde::Deserialize;

use crate::catalog;
use crate::invocation::Invocation;
use crate::keystroke::{Keystroke, Platform};

/// The built-in keymap, in the same format as a user's `keymap.toml`.
pub const DEFAULT_KEYMAP: &str = include_str!("default-keymap.toml");

/// Which keymap a binding comes from, lowest priority first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Layer {
    Default,
    User,
}

impl fmt::Display for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Default => "default keymap",
            Self::User => "user keymap",
        })
    }
}

/// An effective key binding.
#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
    pub keys: Vec<Keystroke>,
    /// The UI context the binding is limited to (`"Editor"`), or `None` for a global binding.
    pub context: Option<String>,
    pub invocation: Invocation,
    pub layer: Layer,
}

impl Binding {
    /// The keys in the textual form GPUI parses, e.g. `ctrl-k ctrl-c`.
    pub fn keys_string(&self) -> String {
        join_keys(&self.keys)
    }

    /// The keys as Linux reports them, if Shift with a digit or punctuation key makes them
    /// differ (see [`Keystroke::shifted_symbol`]).
    pub fn shifted_symbol_keys(&self) -> Option<String> {
        let shifted: Vec<Option<Keystroke>> =
            self.keys.iter().map(Keystroke::shifted_symbol).collect();
        if shifted.iter().all(Option::is_none) {
            return None;
        }
        let keys: Vec<Keystroke> = self
            .keys
            .iter()
            .zip(shifted)
            .map(|(key, shifted)| shifted.unwrap_or_else(|| key.clone()))
            .collect();
        Some(join_keys(&keys))
    }
}

/// A keymap file could not be parsed at all.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct KeymapError(String);

/// A problem found while building a keymap. The offending binding is skipped or overridden;
/// everything else still applies.
#[derive(Debug, Clone, PartialEq)]
pub enum KeymapDiagnostic {
    /// The file is not valid TOML or not shaped like a keymap; the whole layer is ignored.
    Syntax { layer: Layer, message: String },
    /// One binding is malformed: bad keys, unknown command or platform, missing command.
    Invalid {
        layer: Layer,
        keys: String,
        message: String,
    },
    /// The same layer binds the same keys in the same context twice; the later binding wins.
    Duplicate {
        layer: Layer,
        keys: String,
        context: Option<String>,
        replaced: Invocation,
        by: Invocation,
    },
}

impl fmt::Display for KeymapDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax { layer, message } => write!(f, "{layer}: {message}"),
            Self::Invalid {
                layer,
                keys,
                message,
            } => write!(f, "{layer}: `{keys}`: {message}"),
            Self::Duplicate {
                layer,
                keys,
                context,
                replaced,
                by,
            } => write!(
                f,
                "{layer}: `{keys}`{} is bound twice; `{}` replaces `{}`",
                context
                    .as_deref()
                    .map(|c| format!(" in {c}"))
                    .unwrap_or_default(),
                by.command,
                replaced.command,
            ),
        }
    }
}

#[derive(Debug, Deserialize)]
struct KeymapFile {
    #[serde(default)]
    binding: Vec<RawBinding>,
}

#[derive(Debug, Deserialize)]
struct RawBinding {
    keys: String,
    command: Option<String>,
    args: Option<toml::Value>,
    context: Option<String>,
    platforms: Option<Vec<String>>,
    #[serde(default)]
    unbind: bool,
}

/// The effective bindings for one platform.
#[derive(Debug, Clone)]
pub struct Keymap {
    platform: Platform,
    bindings: Vec<Binding>,
    diagnostics: Vec<KeymapDiagnostic>,
}

impl Keymap {
    /// An empty keymap.
    pub fn new(platform: Platform) -> Self {
        Self {
            platform,
            bindings: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    /// The built-in bindings for `platform`.
    pub fn with_defaults(platform: Platform) -> Self {
        let mut keymap = Self::new(platform);
        keymap.add_layer(Layer::Default, DEFAULT_KEYMAP);
        keymap
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }

    /// Effective bindings, without duplicates.
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    pub fn diagnostics(&self) -> &[KeymapDiagnostic] {
        &self.diagnostics
    }

    /// Merges a keymap file on top of the current bindings.
    pub fn add_layer(&mut self, layer: Layer, source: &str) {
        let file = match parse_keymap(source) {
            Ok(file) => file,
            Err(error) => {
                self.diagnostics.push(KeymapDiagnostic::Syntax {
                    layer,
                    message: error.0,
                });
                return;
            }
        };
        for raw in file.binding {
            self.add_raw(layer, raw);
        }
    }

    fn add_raw(&mut self, layer: Layer, raw: RawBinding) {
        let invalid = |message: String| KeymapDiagnostic::Invalid {
            layer,
            keys: raw.keys.clone(),
            message,
        };

        if let Some(platforms) = &raw.platforms {
            if let Some(unknown) = platforms
                .iter()
                .find(|name| !["windows", "linux", "macos"].contains(&name.as_str()))
            {
                self.diagnostics
                    .push(invalid(format!("unknown platform `{unknown}`")));
                return;
            }
            if !platforms.iter().any(|name| name == self.platform.name()) {
                return;
            }
        }

        let keys = match Keystroke::parse_sequence(&raw.keys, self.platform) {
            Ok(keys) => keys,
            Err(error) => {
                self.diagnostics.push(invalid(error.to_string()));
                return;
            }
        };
        let existing = self
            .bindings
            .iter()
            .position(|binding| binding.keys == keys && binding.context == raw.context);

        if raw.unbind {
            if let Some(index) = existing {
                self.bindings.remove(index);
            }
            return;
        }

        let Some(command) = raw.command.as_deref() else {
            self.diagnostics.push(invalid(
                "a binding needs a `command` (or `unbind = true`)".into(),
            ));
            return;
        };
        if catalog::find(command).is_none() {
            self.diagnostics
                .push(invalid(format!("unknown command `{command}`")));
            return;
        }
        let args = match raw.args.map(serde_json::to_value).transpose() {
            Ok(args) => args.unwrap_or_default(),
            Err(error) => {
                self.diagnostics
                    .push(invalid(format!("arguments are not valid JSON: {error}")));
                return;
            }
        };
        let binding = Binding {
            keys,
            context: raw.context,
            invocation: Invocation {
                command: command.to_owned(),
                args,
            },
            layer,
        };

        if let Some(index) = existing {
            let replaced = self.bindings.remove(index);
            if replaced.layer == layer {
                self.diagnostics.push(KeymapDiagnostic::Duplicate {
                    layer,
                    keys: binding.keys_string(),
                    context: binding.context.clone(),
                    replaced: replaced.invocation,
                    by: binding.invocation.clone(),
                });
            }
        }
        self.bindings.push(binding);
    }

    /// The binding a key sequence triggers when the focused element sits under `context_path`
    /// (outermost context first, e.g. `["Workspace", "Editor"]`).
    ///
    /// As in GPUI, a binding for a deeper context wins over a shallower or global one.
    pub fn resolve(&self, keys: &[Keystroke], context_path: &[&str]) -> Option<&Binding> {
        self.bindings
            .iter()
            .filter(|binding| binding.keys == keys)
            .filter_map(|binding| {
                let depth = match &binding.context {
                    None => Some(0),
                    Some(context) => context_path
                        .iter()
                        .rposition(|c| c == context)
                        .map(|i| i + 1),
                };
                depth.map(|depth| (depth, binding))
            })
            .max_by_key(|(depth, _)| *depth)
            .map(|(_, binding)| binding)
    }

    /// Bindings that trigger exactly `invocation`, e.g. to show a shortcut next to a menu item.
    pub fn bindings_for<'a>(
        &'a self,
        invocation: &'a Invocation,
    ) -> impl Iterator<Item = &'a Binding> + 'a {
        self.bindings
            .iter()
            .filter(move |binding| &binding.invocation == invocation)
    }

    /// Pairs of bindings in the same context where one key sequence is a prefix of the other.
    /// The shorter one only fires after a timeout, which usually is not what the user wants.
    pub fn prefix_conflicts(&self) -> Vec<(&Binding, &Binding)> {
        let mut conflicts = Vec::new();
        for short in &self.bindings {
            for long in &self.bindings {
                if short.context == long.context
                    && short.keys.len() < long.keys.len()
                    && long.keys.starts_with(&short.keys)
                {
                    conflicts.push((short, long));
                }
            }
        }
        conflicts
    }
}

fn parse_keymap(source: &str) -> Result<KeymapFile, KeymapError> {
    toml::from_str(source).map_err(|error| KeymapError(error.message().to_owned()))
}

fn join_keys(keys: &[Keystroke]) -> String {
    keys.iter()
        .map(Keystroke::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLATFORMS: [Platform; 3] = [Platform::Windows, Platform::Linux, Platform::MacOs];

    fn keys(source: &str, platform: Platform) -> Vec<Keystroke> {
        Keystroke::parse_sequence(source, platform).unwrap()
    }

    fn command_for<'a>(keymap: &'a Keymap, source: &str, context_path: &[&str]) -> Option<&'a str> {
        keymap
            .resolve(&keys(source, keymap.platform()), context_path)
            .map(|binding| binding.invocation.command.as_str())
    }

    #[test]
    fn default_keymap_is_clean_on_every_platform() {
        for platform in PLATFORMS {
            let keymap = Keymap::with_defaults(platform);
            assert!(
                keymap.diagnostics().is_empty(),
                "{platform:?}: {:#?}",
                keymap.diagnostics()
            );
            assert!(keymap.prefix_conflicts().is_empty());
            assert!(keymap.bindings().len() > 60);
        }
    }

    #[test]
    fn default_bindings_follow_notepad_plus_plus() {
        let windows = Keymap::with_defaults(Platform::Windows);
        let editor = ["Workspace", "Editor"];
        assert_eq!(command_for(&windows, "ctrl-s", &editor), Some("file.save"));
        assert_eq!(
            command_for(&windows, "ctrl-alt-s", &editor),
            Some("file.save-as")
        );
        assert_eq!(
            command_for(&windows, "ctrl-left", &editor),
            Some("cursor.word-left")
        );
        assert_eq!(
            command_for(&windows, "ctrl-tab", &editor),
            Some("view.next-tab")
        );
        // Editing keys only apply inside an editor.
        assert_eq!(command_for(&windows, "ctrl-z", &["Workspace"]), None);

        let mac = Keymap::with_defaults(Platform::MacOs);
        assert_eq!(command_for(&mac, "cmd-s", &editor), Some("file.save"));
        assert_eq!(command_for(&mac, "cmd-left", &editor), Some("cursor.home"));
        assert_eq!(
            command_for(&mac, "alt-backspace", &editor),
            Some("edit.delete-word-left")
        );
        assert_eq!(
            command_for(&windows, "alt-backspace", &editor),
            Some("edit.undo")
        );
    }

    #[test]
    fn every_default_binding_targets_a_known_command() {
        for platform in PLATFORMS {
            for binding in Keymap::with_defaults(platform).bindings() {
                assert!(catalog::find(&binding.invocation.command).is_some());
            }
        }
    }

    #[test]
    fn user_layer_overrides_and_unbinds() {
        let mut keymap = Keymap::with_defaults(Platform::Linux);
        keymap.add_layer(
            Layer::User,
            r#"
                [[binding]]
                keys = "ctrl-s"
                command = "file.save-all"

                [[binding]]
                keys = "ctrl-w"
                unbind = true

                [[binding]]
                keys = "ctrl-shift-l"
                command = "edit.convert-eol"
                args = { eol = "lf" }
                context = "Editor"
            "#,
        );
        assert!(
            keymap.diagnostics().is_empty(),
            "{:?}",
            keymap.diagnostics()
        );
        let path = ["Workspace", "Editor"];
        assert_eq!(command_for(&keymap, "ctrl-s", &path), Some("file.save-all"));
        assert_eq!(command_for(&keymap, "ctrl-w", &path), None);
        let eol = keymap
            .resolve(&keys("ctrl-shift-l", Platform::Linux), &path)
            .unwrap();
        assert_eq!(eol.invocation.args, serde_json::json!({ "eol": "lf" }));
        assert_eq!(eol.layer, Layer::User);

        let save_all = Invocation::new("file.save-all");
        let shortcuts: Vec<String> = keymap
            .bindings_for(&save_all)
            .map(Binding::keys_string)
            .collect();
        assert_eq!(shortcuts, ["ctrl-shift-s", "ctrl-s"]);
    }

    #[test]
    fn deeper_context_wins() {
        let mut keymap = Keymap::new(Platform::Windows);
        keymap.add_layer(
            Layer::Default,
            r#"
                [[binding]]
                keys = "tab"
                command = "view.next-tab"

                [[binding]]
                keys = "tab"
                command = "edit.tab"
                context = "Editor"
            "#,
        );
        assert_eq!(
            command_for(&keymap, "tab", &["Workspace", "Editor"]),
            Some("edit.tab")
        );
        assert_eq!(
            command_for(&keymap, "tab", &["Workspace"]),
            Some("view.next-tab")
        );
    }

    #[test]
    fn reports_problems_and_keeps_going() {
        let mut keymap = Keymap::new(Platform::Windows);
        keymap.add_layer(
            Layer::User,
            r#"
                [[binding]]
                keys = "ctrl-q"
                command = "no.such-command"

                [[binding]]
                keys = "hyper-q"
                command = "file.new"

                [[binding]]
                keys = "ctrl-e"

                [[binding]]
                keys = "ctrl-e"
                command = "file.new"
                platforms = ["amiga"]

                [[binding]]
                keys = "ctrl-k"
                command = "file.new"

                [[binding]]
                keys = "ctrl-k"
                command = "file.open"

                [[binding]]
                keys = "ctrl-k ctrl-c"
                command = "file.close"
            "#,
        );
        let diagnostics = keymap.diagnostics();
        assert_eq!(diagnostics.len(), 5, "{diagnostics:#?}");
        assert!(matches!(
            &diagnostics[4],
            KeymapDiagnostic::Duplicate { by, .. } if by.command == "file.open"
        ));
        assert_eq!(
            command_for(&keymap, "ctrl-k", &[]),
            Some("file.open"),
            "the later duplicate wins"
        );
        assert_eq!(keymap.prefix_conflicts().len(), 1);

        let mut broken = Keymap::new(Platform::Windows);
        broken.add_layer(Layer::User, "[[binding]\nkeys = ");
        assert!(matches!(
            broken.diagnostics(),
            [KeymapDiagnostic::Syntax { .. }]
        ));
    }

    #[test]
    fn problems_read_as_sentences() {
        assert_eq!(Layer::Default.to_string(), "default keymap");
        assert_eq!(Layer::User.to_string(), "user keymap");
        let syntax = KeymapDiagnostic::Syntax {
            layer: Layer::User,
            message: "expected `]`".into(),
        };
        assert_eq!(syntax.to_string(), "user keymap: expected `]`");
        let invalid = KeymapDiagnostic::Invalid {
            layer: Layer::Default,
            keys: "hyper-q".into(),
            message: "unknown modifier or key `hyper`".into(),
        };
        assert_eq!(
            invalid.to_string(),
            "default keymap: `hyper-q`: unknown modifier or key `hyper`"
        );
        let duplicate = |context: Option<&str>| KeymapDiagnostic::Duplicate {
            layer: Layer::User,
            keys: "ctrl-k".into(),
            context: context.map(str::to_owned),
            replaced: Invocation::new("file.new"),
            by: Invocation::new("file.open"),
        };
        assert_eq!(
            duplicate(None).to_string(),
            "user keymap: `ctrl-k` is bound twice; `file.open` replaces `file.new`"
        );
        assert_eq!(
            duplicate(Some("Editor")).to_string(),
            "user keymap: `ctrl-k` in Editor is bound twice; `file.open` replaces `file.new`"
        );
    }

    #[test]
    fn an_empty_user_keymap_changes_nothing() {
        let defaults = Keymap::with_defaults(Platform::Windows);
        let mut keymap = Keymap::with_defaults(Platform::Windows);
        keymap.add_layer(Layer::User, "");
        assert!(keymap.diagnostics().is_empty());
        assert_eq!(keymap.bindings().len(), defaults.bindings().len());
    }

    #[test]
    fn platform_specific_bindings_are_filtered() {
        let linux = Keymap::with_defaults(Platform::Linux);
        let mac = Keymap::with_defaults(Platform::MacOs);
        assert!(command_for(&linux, "cmd-q", &[]).is_none());
        assert_eq!(command_for(&mac, "cmd-q", &[]), Some("file.exit"));
    }
}
