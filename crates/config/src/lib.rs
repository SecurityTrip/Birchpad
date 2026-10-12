//! Layered settings for Birchpad.
//!
//! Effective settings are built from four layers, each overriding the previous one:
//! built-in defaults, machine defaults set by an administrator, the user's settings, and
//! administrator policies. Keys set by policy are locked: the UI shows them read-only.

mod api;
mod layers;
mod load;
mod notepad_session;
mod policy;
mod project;
mod session;
mod settings;
mod state;
mod write;

pub use api::{ApiError, ApiFile, ApiKeyword, ApiOverload};
pub use layers::{Diagnostic, Layer, ResolvedSettings, Sources, resolve};
pub use load::{ConfigPaths, PORTABLE_DATA_DIR, PORTABLE_MARKER, load, load_platform_settings};
pub use policy::{POLICIES, PolicyDef, PolicyKind, REGISTRY_KEY};
pub use project::{ProjectError, ProjectFolder, ProjectItem, ProjectWorkspace};
pub use session::{
    SESSION_VERSION, Session, SessionDocument, SessionError, SessionTab, SessionView,
    create_private_dir, write_private,
};
pub use settings::{
    AppearanceSettings, AutoIndent, ChangeDetection, ChangeHistory, CompletionSettings,
    CurrentLine, DEFAULT_TOOLBAR, DateSettings, DiagnosticsSettings, Edge, EditorSettings,
    FileSettings, HighlightingSettings, InstanceMode, InstanceSettings, NetworkSettings,
    PluginInstall, PluginSettings, SearchEngine, SearchEngineSettings, SessionSettings, Settings,
    SmartHighlighting, TabSettings, TokenMatching, ToolbarSettings, ToolbarSize, UpdateChannel,
    UpdateMode, UpdateSettings,
};
pub use state::{
    FindInFilesState, MAX_RECENT_FILES, PanelsState, SpellCheckState, SplitOrientation,
    UpdateState, UserState,
};
pub use write::{SettingError, check_setting, edit_settings, write_setting};
