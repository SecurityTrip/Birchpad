//! Preferences > Appearance and Toolbar: the interface's language, Dark Mode, the bars shown.

use serde::{Deserialize, Serialize};

/// How Birchpad looks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct AppearanceSettings {
    /// The color theme as Birchpad starts: a built-in one (`Default`, `Dark`) or the name of a
    /// file in the `themes` folder next to the settings, without its extension. Settings > Theme
    /// chooses another (remembered in `state.toml`).
    pub theme: String,
    /// The interface's language: `en`, `de`, `ru`, or `auto` for the system's.
    pub language: String,
    /// A Notepad++ translation (`localization/german.xml` of a Notepad++ folder) whose menu
    /// names and command titles come before the language's own; empty for none.
    pub native_lang: String,
    /// The title bar shows the file's name, not its whole path ("Show only filename in title
    /// bar").
    pub title_file_name_only: bool,
    /// The colors of menus, tabs, panels and dialogs, apart from the editor's theme: Notepad++'s
    /// Preferences > Dark Mode.
    pub interface: InterfaceMode,
    /// A hint of color in the dark interface (Dark Mode's tones).
    pub dark_tone: DarkTone,
    /// Dark Mode's customized tone: colors of the dark interface, over its own while the tone
    /// is `custom`.
    pub dark_tone_colors: DarkToneColors,
    /// The menu bar shows; hidden, Alt or F10 shows it while a menu is open ("Hide menu bar").
    pub menu_bar: bool,
    /// The status bar shows ("Status Bar > Hide" off).
    pub status_bar: bool,
}

/// Notepad++'s Dark Mode tones: a hint of color in the dark interface's surfaces.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DarkTone {
    #[default]
    Black,
    Red,
    Green,
    Blue,
    Purple,
    Cyan,
    Olive,
    /// The colors of [`DarkToneColors`] ("Customized tone").
    Custom,
}

impl DarkTone {
    /// The hue of the tone, in degrees; none for black and the customized tone.
    pub fn hue(self) -> Option<f32> {
        match self {
            Self::Black | Self::Custom => None,
            Self::Red => Some(0.),
            Self::Olive => Some(60.),
            Self::Green => Some(120.),
            Self::Cyan => Some(180.),
            Self::Blue => Some(220.),
            Self::Purple => Some(280.),
        }
    }
}

/// The colors of Dark Mode's customized tone, each `#rrggbb`, or empty for the dark interface's
/// own: its parts as theme files name them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct DarkToneColors {
    /// Panels, lists and tabs.
    #[serde(deserialize_with = "hex_color")]
    pub background: String,
    /// Bars and headings.
    #[serde(deserialize_with = "hex_color")]
    pub surface: String,
    #[serde(deserialize_with = "hex_color")]
    pub border: String,
    #[serde(deserialize_with = "hex_color")]
    pub text: String,
    /// Secondary text.
    #[serde(deserialize_with = "hex_color")]
    pub muted: String,
    /// Disabled text.
    #[serde(deserialize_with = "hex_color")]
    pub faint: String,
    /// The selected row of a list.
    #[serde(deserialize_with = "hex_color")]
    pub selected: String,
    #[serde(deserialize_with = "hex_color")]
    pub hovered: String,
    /// The active view.
    #[serde(deserialize_with = "hex_color")]
    pub accent: String,
    #[serde(deserialize_with = "hex_color")]
    pub error: String,
}

impl DarkToneColors {
    /// The colors set, by their keys (`background`).
    pub fn set(&self) -> impl Iterator<Item = (&'static str, &str)> {
        [
            ("background", &self.background),
            ("surface", &self.surface),
            ("border", &self.border),
            ("text", &self.text),
            ("muted", &self.muted),
            ("faint", &self.faint),
            ("selected", &self.selected),
            ("hovered", &self.hovered),
            ("accent", &self.accent),
            ("error", &self.error),
        ]
        .into_iter()
        .filter(|(_, color)| !color.is_empty())
        .map(|(key, color)| (key, color.as_str()))
    }
}

/// A color written `#rrggbb`, or empty.
pub(super) fn hex_color<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<String, D::Error> {
    let text = String::deserialize(deserializer)?;
    let digits = text.strip_prefix('#').unwrap_or_default();
    if text.is_empty() || (digits.len() == 6 && digits.bytes().all(|b| b.is_ascii_hexdigit())) {
        Ok(text)
    } else {
        Err(serde::de::Error::custom(format!(
            "{text:?} is not a color written #rrggbb"
        )))
    }
}

/// Notepad++'s Dark Mode for the interface.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InterfaceMode {
    /// The theme's own interface colors.
    #[default]
    Theme,
    Light,
    Dark,
    /// Light or dark as the system is ("Follow Windows").
    System,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme: "Default".to_owned(),
            language: "auto".to_owned(),
            native_lang: String::new(),
            title_file_name_only: false,
            interface: InterfaceMode::default(),
            dark_tone: DarkTone::default(),
            dark_tone_colors: DarkToneColors::default(),
            menu_bar: true,
            status_bar: true,
        }
    }
}

/// The toolbar under the menus, as Notepad++'s (Preferences > General > Toolbar).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ToolbarSettings {
    /// Whether it shows.
    pub show: bool,
    pub size: ToolbarSize,
    /// Its buttons, in order: command ids, and `|` for a separator. Commands it does not know
    /// are left out.
    pub buttons: Vec<String>,
}

/// The toolbar's buttons as Notepad++ has them.
pub const DEFAULT_TOOLBAR: &[&str] = &[
    "file.new",
    "file.open",
    "file.save",
    "file.save-all",
    "file.close",
    "file.close-all",
    "file.print",
    "|",
    "edit.cut",
    "edit.copy",
    "edit.paste",
    "|",
    "edit.undo",
    "edit.redo",
    "|",
    "search.find",
    "search.replace",
    "|",
    "view.zoom-in",
    "view.zoom-out",
    "|",
    "view.sync-vertical-scroll",
    "view.sync-horizontal-scroll",
    "|",
    "view.word-wrap",
    "view.show-all-characters",
    "view.indent-guides",
    "|",
    "panel.document-map",
    "panel.document-list",
    "panel.function-list",
    "panel.folder-as-workspace",
    "|",
    "view.monitoring",
    "|",
    "macro.start-recording",
    "macro.stop-recording",
    "macro.playback",
    "macro.run-multiple",
    "macro.save",
];

impl Default for ToolbarSettings {
    fn default() -> Self {
        Self {
            show: true,
            size: ToolbarSize::default(),
            buttons: DEFAULT_TOOLBAR.iter().map(|&id| id.to_owned()).collect(),
        }
    }
}

/// How big the toolbar's icons are.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolbarSize {
    #[default]
    Small,
    Large,
}
