//! Layered settings for Birchpad.
//!
//! Effective settings are built from four layers, each overriding the previous one:
//! built-in defaults, machine defaults set by an administrator, the user's settings, and
//! administrator policies. Keys set by policy are locked: the UI shows them read-only.

mod api;
mod layers;
mod load;
mod native_lang;
mod notepad_session;
mod policy;
mod project;
mod session;
mod session_names;
mod settings;
mod state;
mod write;

pub use api::{ApiError, ApiFile, ApiKeyword, ApiOverload};
pub use layers::{Diagnostic, Layer, ResolvedSettings, Sources, resolve};
pub use load::{ConfigPaths, PORTABLE_DATA_DIR, PORTABLE_MARKER, load, load_platform_settings};
pub use native_lang::{COMMANDS as NATIVE_LANG_COMMANDS, NativeLang, parse_native_lang};
#[cfg(windows)]
pub use policy::registry::user_registry_policies;
pub use policy::{POLICIES, PolicyDef, PolicyKind, REGISTRY_KEY};
pub use project::{ProjectError, ProjectFolder, ProjectItem, ProjectWorkspace};
pub use session::{
    SESSION_VERSION, Session, SessionDocument, SessionError, SessionTab, SessionView,
    create_private_dir, write_private,
};
pub use session_names::{
    LanguageName, USER_LANGUAGE_PREFIX, backup_name, free_untitled_number, line_ending_name,
    parse_line_ending,
};
pub use settings::{
    AppearanceSettings, AutoIndent, BackupOnSave, CaretWidth, ChangeDetection, ChangeHistory,
    CloudSettings, CompareSettings, CompletionSettings, CurrentLine, DEFAULT_TOOLBAR, DarkTone,
    DateSettings, DefaultDirectory, DelimiterSettings, DiagnosticsSettings, Edge, EditorSettings,
    EolStyle, FileSettings, FinderSettings, FoldStyle, HexSettings, HighlightingSettings,
    InstanceMode, InstanceSettings, InterfaceMode, LanguageSettings, LineNumberWidth, LinkSettings,
    NetworkSettings, NewDocumentSettings, PaletteSettings, Paper, PluginInstall, PluginSettings,
    PreviewSettings, PrintColors, PrintSettings, SearchEngine, SearchEngineSettings,
    SessionSettings, Settings, SmartHighlighting, SpellScope, SpellSettings, TabSettings,
    TokenMatching, ToolbarSettings, ToolbarSize, UpdateChannel, UpdateMode, UpdateSettings,
    WrapIndent,
};
pub use state::{
    FindInFilesState, MAX_RECENT_COMMANDS, MAX_RECENT_FILES, PanelsState, PreviewMode,
    SearchResultsState, SpellCheckState, SplitOrientation, UpdateState, UserState,
};
pub use write::{SettingError, check_setting, edit_settings, write_setting};
