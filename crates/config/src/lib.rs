//! Layered settings for Birchpad.
//!
//! Effective settings are built from four layers, each overriding the previous one:
//! built-in defaults, machine defaults set by an administrator, the user's settings, and
//! administrator policies. Keys set by policy are locked: the UI shows them read-only.

mod layers;
mod load;
mod notepad_session;
mod policy;
mod project;
mod session;
mod settings;
mod state;

pub use layers::{Diagnostic, Layer, ResolvedSettings, Sources, resolve};
pub use load::{ConfigPaths, PORTABLE_DATA_DIR, PORTABLE_MARKER, load, load_platform_settings};
pub use policy::{POLICIES, PolicyDef, PolicyKind, REGISTRY_KEY};
pub use project::{ProjectError, ProjectFolder, ProjectItem, ProjectWorkspace};
pub use session::{
    SESSION_VERSION, Session, SessionDocument, SessionError, SessionTab, SessionView,
    create_private_dir, write_private,
};
pub use settings::{
    AutoIndent, ChangeDetection, ChangeHistory, CurrentLine, DiagnosticsSettings, Edge,
    EditorSettings, FileSettings, HighlightingSettings, NetworkSettings, PluginInstall,
    PluginSettings, SessionSettings, Settings, SmartHighlighting, TokenMatching, UpdateChannel,
    UpdateMode, UpdateSettings,
};
pub use state::{
    FindInFilesState, MAX_RECENT_FILES, PanelsState, SplitOrientation, UpdateState, UserState,
};
