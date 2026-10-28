//! Preferences > Print and Preview.

use serde::{Deserialize, Serialize};

/// The previews beside the text (View > Preview).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct PreviewSettings {
    /// What draws PlantUML diagrams: the `plantuml` command (found on `PATH` by its name, or
    /// a path to it), or a path to `plantuml.jar`, run with `java -jar`. With `plantuml`, the
    /// default, the PlantUML Birchpad installed (View > Preview > Install PlantUML) runs when
    /// there is one.
    pub plantuml: String,
    /// What PlantUML diagrams are drawn on in the preview: white, as PlantUML draws them for
    /// paper (the default), or the theme's background.
    pub plantuml_background: DiagramBackground,
    /// How many times its size a diagram copied or exported as PNG is drawn, from 1 to 8.
    pub plantuml_png_scale: u32,
}

impl Default for PreviewSettings {
    fn default() -> Self {
        Self {
            plantuml: "plantuml".to_owned(),
            plantuml_background: DiagramBackground::default(),
            plantuml_png_scale: 2,
        }
    }
}

/// What the preview draws PlantUML diagrams on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiagramBackground {
    /// White, whatever the theme: diagrams are made for paper.
    #[default]
    White,
    /// The theme's background, as the text around it.
    Theme,
}

/// File > Print... and Print Now: the page and what is on it (Notepad++'s Preferences > Print,
/// with the page setup of Birchpad's own print preview).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct PrintSettings {
    /// The line numbers in front of the lines ("Print line number").
    pub line_numbers: bool,
    pub colors: PrintColors,
    pub paper: Paper,
    pub landscape: bool,
    /// The margins from the paper's edges, in millimeters.
    pub margin_top: u32,
    pub margin_bottom: u32,
    pub margin_left: u32,
    pub margin_right: u32,
    /// The text's size in points.
    pub font_size: u32,
    /// The header's and the footer's three parts, with Notepad++'s variables
    /// (`$(FULL_CURRENT_PATH)`, `$(FILE_NAME)`, `$(CURRENT_DIRECTORY)`,
    /// `$(CURRENT_PRINTING_PAGE)`, `$(SHORT_DATE)`, `$(LONG_DATE)`, `$(TIME)`); empty parts
    /// print nothing, and a header or footer whose parts are all empty takes no room.
    pub header_left: String,
    pub header_center: String,
    pub header_right: String,
    pub footer_left: String,
    pub footer_center: String,
    pub footer_right: String,
    /// A form feed (U+000C) starts a new page ("Print formfeed as page break").
    pub form_feed_page_break: bool,
}

impl Default for PrintSettings {
    fn default() -> Self {
        Self {
            line_numbers: true,
            colors: PrintColors::default(),
            paper: Paper::default(),
            landscape: false,
            margin_top: 15,
            margin_bottom: 15,
            margin_left: 15,
            margin_right: 15,
            font_size: 10,
            header_left: String::new(),
            header_center: String::new(),
            header_right: String::new(),
            footer_left: String::new(),
            footer_center: String::new(),
            footer_right: String::new(),
            form_feed_page_break: false,
        }
    }
}

/// The colors printed (Notepad++'s Colour Options).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PrintColors {
    /// As on screen, backgrounds too ("WYSIWYG").
    #[default]
    Wysiwyg,
    /// The screen's colors inverted.
    Invert,
    /// Black text on white paper, styles kept.
    BlackOnWhite,
    /// The text's colors on white paper.
    NoBackground,
}

/// The paper printed on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Paper {
    A3,
    #[default]
    A4,
    A5,
    Letter,
    Legal,
}
