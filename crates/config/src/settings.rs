//! The settings schema. Every field has a default, so any layer may set any subset of keys,
//! and unknown keys are ignored so that settings written by a newer version still load.

mod appearance;
mod completion;
mod editor;
mod files;
mod git;
mod highlighting;
mod instances;
mod languages;
mod links;
mod lsp;
mod misc;
mod print;
mod searching;
mod tabs;
mod tools;
mod updates;

pub use appearance::{
    AppearanceSettings, DEFAULT_TOOLBAR, DarkTone, DarkToneColors, InterfaceMode, ToolbarSettings,
    ToolbarSize,
};
pub use completion::CompletionSettings;
pub use editor::{
    AutoIndent, CaretWidth, ChangeHistory, CurrentLine, DelimiterSettings, Edge, EditorSettings,
    EolStyle, FoldStyle, LineNumberWidth, NonPrintingStyle, WrapIndent,
};
pub use files::{
    BackupOnSave, ChangeDetection, DefaultDirectory, FileSettings, NewDocumentSettings,
    SessionSettings,
};
pub use git::GitSettings;
pub use highlighting::{HighlightingSettings, SmartHighlighting, TokenMatching};
pub use instances::{DateSettings, InstanceMode, InstanceSettings};
pub use languages::{LanguageMenuSettings, LanguageSettings, SqlSettings};
pub use links::{CloudSettings, LinkSettings};
pub use lsp::LspSettings;
pub use misc::{CloseToTray, MiscSettings};
pub use print::{
    DiagramBackground, DiagramLayout, Paper, PreviewSettings, PrintColors, PrintSettings,
};
pub use searching::{
    FinderSettings, PaletteSettings, SearchEngine, SearchEngineSettings, SearchingSettings,
};
pub use tabs::TabSettings;
pub use tools::{CompareSettings, HexSettings, SpellScope, SpellSettings};
pub use updates::{
    DiagnosticsSettings, NetworkSettings, PluginInstall, PluginSettings, UpdateChannel, UpdateMode,
    UpdateSettings,
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Settings {
    pub appearance: AppearanceSettings,
    pub editor: EditorSettings,
    pub completion: CompletionSettings,
    pub highlighting: HighlightingSettings,
    pub files: FileSettings,
    pub tabs: TabSettings,
    pub toolbar: ToolbarSettings,
    pub instances: InstanceSettings,
    pub date: DateSettings,
    pub search_engine: SearchEngineSettings,
    pub links: LinkSettings,
    pub delimiters: DelimiterSettings,
    pub new_document: NewDocumentSettings,
    /// Tab size and spaces of a language, by its id (`python`), over `editor`'s
    /// ("Preferences > Language > Tab Settings").
    pub languages: std::collections::BTreeMap<String, LanguageSettings>,
    pub cloud: CloudSettings,
    pub session: SessionSettings,
    pub updates: UpdateSettings,
    pub plugins: PluginSettings,
    pub network: NetworkSettings,
    pub diagnostics: DiagnosticsSettings,
    pub preview: PreviewSettings,
    pub print: PrintSettings,
    pub compare: CompareSettings,
    pub hex: HexSettings,
    pub spell: SpellSettings,
    pub palette: PaletteSettings,
    pub finder: FinderSettings,
    pub searching: SearchingSettings,
    pub misc: MiscSettings,
    pub language_menu: LanguageMenuSettings,
    pub sql: SqlSettings,
    /// Preferences > Language Servers (ADR 0065).
    pub lsp: LspSettings,
    /// Preferences > Git (ADR 0066).
    pub git: GitSettings,
}
