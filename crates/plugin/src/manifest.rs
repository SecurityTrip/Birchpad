//! `plugin.toml`: what a plugin is, and which file holds its component.
//!
//! ```toml
//! id = "word-count"
//! name = "Word Count"
//! version = "1.0.0"
//! description = "Counts the words of the document or the selection."
//! author = "Jane Doe"
//! api = "0.2"
//! events = ["saved"]
//! permissions = ["files"]
//!
//! [[settings]]
//! key = "count-numbers"
//! title = "Count numbers as words"
//! kind = "bool"
//! default = "true"
//! ```

use serde::{Deserialize, Serialize};

/// The version of the plugin API (the WIT package `birchpad:plugin`) this Birchpad provides,
/// as major and minor.
pub const API_VERSION: &str = "0.2";

/// The longest plugin id accepted, in bytes.
pub const MAX_ID: usize = 64;
/// The longest plugin name accepted, in characters.
pub const MAX_NAME: usize = 100;
/// The most settings a manifest may declare.
pub const MAX_DECLARED_SETTINGS: usize = 64;

/// An event a plugin may ask to hear about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EventKind {
    Opened,
    Saved,
    Closed,
    Activated,
    /// The text changed (`on-event`).
    Changed,
    /// The selections moved (`on-event`).
    SelectionChanged,
    /// The command line's `-pluginMessage` (`on-message`).
    Message,
}

/// What a plugin may ask the user to let it do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Permission {
    /// Read and write files, and open them in tabs.
    Files,
}

/// How Preferences > Plugins edits a setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SettingKind {
    /// A check box: `true` or `false`.
    Bool,
    /// A whole number.
    Number,
    Text,
}

/// A setting a plugin shows in Preferences > Plugins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingSpec {
    pub key: String,
    /// Its label.
    pub title: String,
    pub kind: SettingKind,
    /// Its value until the user or the plugin sets it.
    #[serde(default)]
    pub default: String,
}

impl SettingSpec {
    /// Whether `value` is one the setting can take.
    pub fn accepts(&self, value: &str) -> bool {
        match self.kind {
            SettingKind::Bool => matches!(value, "true" | "false"),
            SettingKind::Number => value.parse::<i64>().is_ok(),
            SettingKind::Text => !value.contains(['\n', '\r']),
        }
    }
}

/// A plugin's `plugin.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Unique among plugins: lower-case letters, digits and dashes. It names the plugin's
    /// folder and is what an administrator's allowlist holds.
    pub id: String,
    /// Shown in the Plugins menu and in Plugins Admin.
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    /// The plugin API version the plugin was built for.
    pub api: String,
    /// The component's file, next to `plugin.toml`.
    #[serde(default = "default_entry")]
    pub entry: String,
    /// The events the plugin's `on-event` (and `on-message`) is called for.
    #[serde(default)]
    pub events: Vec<EventKind>,
    /// What the plugin asks the user to let it do.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<Permission>,
    /// The settings Preferences > Plugins shows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub settings: Vec<SettingSpec>,
}

fn default_entry() -> String {
    "plugin.wasm".to_owned()
}

/// A manifest that does not read or does not make sense.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ManifestError {
    #[error("{}", birchpad_i18n::t!("plugin.manifest-does-not-read", reason = .0))]
    Syntax(String),
    #[error("{0}")]
    Invalid(String),
}

/// Whether `id` is a valid plugin id (also used for the ids of a plugin's commands).
pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !id.starts_with('-')
        && !id.ends_with('-')
}

impl Manifest {
    /// Reads and checks a `plugin.toml`.
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        let manifest: Self =
            toml::from_str(text).map_err(|error| ManifestError::Syntax(error.message().into()))?;
        manifest.check()?;
        Ok(manifest)
    }

    fn check(&self) -> Result<(), ManifestError> {
        let invalid = |message: String| Err(ManifestError::Invalid(message));
        if !is_valid_id(&self.id) {
            return invalid(format!(
                "{:?} is not a plugin id: use lower-case letters, digits and dashes, at most {MAX_ID}",
                self.id
            ));
        }
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > MAX_NAME || name.contains(char::is_control) {
            return invalid(format!(
                "the plugin {} needs a name of one line, at most {MAX_NAME} characters",
                self.id
            ));
        }
        if self.version.trim().is_empty() {
            return invalid(format!("the plugin {} needs a version", self.id));
        }
        if self.api != API_VERSION {
            return invalid(format!(
                "the plugin {} is built for plugin API {}, and Birchpad provides {API_VERSION}",
                self.id, self.api
            ));
        }
        let entry = &self.entry;
        let plain = !entry.is_empty()
            && !entry.contains(['/', '\\', ':'])
            && !entry.starts_with('.')
            && entry.ends_with(".wasm");
        if !plain {
            return invalid(format!(
                "the entry {entry:?} must be a .wasm file next to plugin.toml"
            ));
        }
        if self.settings.len() > MAX_DECLARED_SETTINGS {
            return invalid(format!(
                "the plugin {} declares {} settings, more than {MAX_DECLARED_SETTINGS}",
                self.id,
                self.settings.len()
            ));
        }
        for (index, setting) in self.settings.iter().enumerate() {
            if !crate::session::is_setting_key(&setting.key) {
                return invalid(format!(
                    "{:?} is not a setting key: use lower-case letters, digits, dashes, dots and underscores",
                    setting.key
                ));
            }
            if self.settings[..index]
                .iter()
                .any(|other| other.key == setting.key)
            {
                return invalid(format!("the setting {} comes twice", setting.key));
            }
            let title = setting.title.trim();
            if title.is_empty() || title.contains(char::is_control) {
                return invalid(format!(
                    "the setting {} needs a title of one line",
                    setting.key
                ));
            }
            let default_fits = match setting.kind {
                // An unset number or switch starts empty: 0 or off.
                SettingKind::Bool | SettingKind::Number => {
                    setting.default.is_empty() || setting.accepts(&setting.default)
                }
                SettingKind::Text => setting.accepts(&setting.default),
            };
            if !default_fits {
                return invalid(format!(
                    "the default {:?} of the setting {} is not a value it takes",
                    setting.default, setting.key
                ));
            }
        }
        Ok(())
    }

    /// Whether the plugin asks for `permission`.
    pub fn asks_for(&self, permission: Permission) -> bool {
        self.permissions.contains(&permission)
    }

    /// The setting `key` the manifest declares.
    pub fn setting(&self, key: &str) -> Option<&SettingSpec> {
        self.settings.iter().find(|setting| setting.key == key)
    }

    /// Writes the manifest back as TOML.
    pub fn to_toml(&self) -> String {
        toml::to_string(self).unwrap_or_default()
    }
}
