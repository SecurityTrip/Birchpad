//! Layered settings for Birchpad.
//!
//! Effective settings are built from four layers, each overriding the previous one:
//! built-in defaults, machine defaults set by an administrator, the user's settings, and
//! administrator policies. Keys set by policy are locked: the UI shows them read-only.

mod layers;
mod load;
mod policy;
mod settings;

pub use layers::{Diagnostic, Layer, ResolvedSettings, Sources, resolve};
pub use load::{ConfigPaths, load, load_platform_settings};
pub use policy::{POLICIES, PolicyDef, PolicyKind, REGISTRY_KEY};
pub use settings::{
    DiagnosticsSettings, EditorSettings, FileSettings, NetworkSettings, PluginInstall,
    PluginSettings, SessionSettings, Settings, UpdateChannel, UpdateMode, UpdateSettings,
};
