//! What the View menu and the settings set for an editor view: wrap, symbols, edges, tabs.

use birchpad_config::{CurrentLine, Edge};
use birchpad_ui::AppState;
use gpui_kit::{App, Pixels, px};

use crate::ZOOM_RANGE;
use crate::buffer::Buffer;

use super::BASE_FONT_SIZE;

/// View menu switches kept in `state.toml`: command, current value, how to remember a new one.
pub type ViewSwitch = (
    &'static str,
    fn(&ViewSettings) -> bool,
    fn(&mut birchpad_config::UserState, bool),
);

pub const VIEW_SWITCHES: &[ViewSwitch] = &[
    (
        "view.word-wrap",
        |view| view.word_wrap,
        |state, on| state.word_wrap = Some(on),
    ),
    (
        "view.show-whitespace",
        |view| view.show_whitespace,
        |state, on| state.show_whitespace = Some(on),
    ),
    (
        "view.show-eol",
        |view| view.show_eol,
        |state, on| state.show_eol = Some(on),
    ),
    (
        "view.indent-guides",
        |view| view.indent_guides,
        |state, on| state.indent_guides = Some(on),
    ),
    (
        "view.wrap-symbol",
        |view| view.wrap_symbol,
        |state, on| state.wrap_symbol = Some(on),
    ),
    (
        "view.show-non-printing",
        |view| view.show_non_printing,
        |state, on| state.show_non_printing = Some(on),
    ),
    (
        "view.show-control-chars",
        |view| view.show_control_chars,
        |state, on| state.show_control_chars = Some(on),
    ),
];

/// View > Show Symbol > Show All Characters: whitespace and line ends both shown.
pub fn all_characters(view: &ViewSettings) -> bool {
    view.show_whitespace && view.show_eol
}

/// The view-related settings, read from the application state on every frame.
#[derive(Debug, Clone)]
pub struct ViewSettings {
    /// The theme's font, else the platform's monospace one.
    pub font_family: gpui_kit::SharedString,
    pub tab_width: usize,
    pub insert_spaces: bool,
    pub word_wrap: bool,
    pub font_size: Pixels,
    pub line_numbers: bool,
    pub bookmark_margin: bool,
    pub fold_margin: bool,
    /// The change history margin.
    pub change_history: bool,
    /// View > Show Symbol.
    pub show_whitespace: bool,
    pub show_eol: bool,
    pub indent_guides: bool,
    pub wrap_symbol: bool,
    pub show_non_printing: bool,
    pub non_printing_style: birchpad_config::NonPrintingStyle,
    pub show_control_chars: bool,
    pub current_line: CurrentLine,
    pub current_line_frame_width: Pixels,
    pub caret_width: birchpad_config::CaretWidth,
    pub scroll_beyond_last_line: bool,
    pub wrap_indent: birchpad_config::WrapIndent,
    pub eol_style: birchpad_config::EolStyle,
    pub fold_style: birchpad_config::FoldStyle,
    /// The line between the margins and the text.
    pub border_width: Pixels,
    pub line_number_width: birchpad_config::LineNumberWidth,
    /// Room left and right of the text, besides the editor's own few pixels.
    pub padding_left: Pixels,
    pub padding_right: Pixels,
    pub edge: Edge,
    pub edge_columns: Vec<usize>,
    /// Sticky scroll, and the most lines it keeps.
    pub sticky_scroll: Option<usize>,
}

impl ViewSettings {
    pub fn read(cx: &App) -> Self {
        let app = AppState::global(cx);
        let editor = &app.settings.editor;
        let zoom = app.state.zoom.clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end());
        let theme_font = crate::theme::current().font.clone();
        Self {
            tab_width: usize::from(editor.tab_width.max(1)),
            insert_spaces: editor.insert_spaces,
            word_wrap: app.state.word_wrap.unwrap_or(editor.word_wrap),
            font_family: theme_font.family().map_or_else(
                || crate::MONOSPACE.into(),
                |family| family.to_owned().into(),
            ),
            font_size: px(theme_font.size().unwrap_or(BASE_FONT_SIZE) + zoom as f32),
            line_numbers: editor.line_numbers,
            bookmark_margin: editor.bookmark_margin,
            fold_margin: editor.fold_margin,
            change_history: editor.change_history == birchpad_config::ChangeHistory::Margin,
            show_whitespace: app.state.show_whitespace.unwrap_or(editor.show_whitespace),
            show_eol: app.state.show_eol.unwrap_or(editor.show_eol),
            indent_guides: app.state.indent_guides.unwrap_or(editor.indent_guides),
            wrap_symbol: app.state.wrap_symbol.unwrap_or(editor.wrap_symbol),
            show_non_printing: app
                .state
                .show_non_printing
                .unwrap_or(editor.show_non_printing),
            non_printing_style: editor.non_printing_style,
            show_control_chars: app
                .state
                .show_control_chars
                .unwrap_or(editor.show_control_chars),
            current_line: editor.current_line,
            caret_width: editor.caret_width,
            scroll_beyond_last_line: editor.scroll_beyond_last_line,
            wrap_indent: editor.wrap_indent,
            eol_style: editor.eol_style,
            fold_style: editor.fold_style,
            border_width: px(f32::from(editor.border_width.min(30))),
            line_number_width: editor.line_number_width,
            padding_left: px(f32::from(editor.padding_left.min(30))),
            padding_right: px(f32::from(editor.padding_right.min(30))),
            current_line_frame_width: px(f32::from(editor.current_line_frame_width.clamp(1, 6))),
            edge: if editor.edge_columns.is_empty() {
                Edge::Off
            } else {
                editor.edge
            },
            edge_columns: editor
                .edge_columns
                .iter()
                .map(|&c| usize::from(c))
                .collect(),
            sticky_scroll: editor.sticky_scroll.then(|| editor.sticky_lines()),
        }
    }

    /// The settings for a document in `language` (an id, `text`, or `udl:<name>`): its tab
    /// size and spaces from `languages.<id>` where set.
    pub fn for_language(language: &str, cx: &App) -> Self {
        let mut settings = Self::read(cx);
        if let Some(own) = AppState::global(cx).settings.languages.get(language) {
            if let Some(width) = own.tab_width {
                settings.tab_width = usize::from(width.max(1));
            }
            if let Some(spaces) = own.insert_spaces {
                settings.insert_spaces = spaces;
            }
        }
        settings
    }

    /// The labels non-printing characters take the cells of: none unless View > Show Symbol >
    /// Show Non-Printing Characters draws them by label.
    pub fn labels(&self) -> birchpad_view::NonPrintingLabel {
        use birchpad_config::NonPrintingStyle;
        use birchpad_view::NonPrintingLabel;
        match self.non_printing_style {
            _ if !self.show_non_printing => NonPrintingLabel::Off,
            NonPrintingStyle::Marks => NonPrintingLabel::Off,
            NonPrintingStyle::Abbreviation => NonPrintingLabel::Abbreviation,
            NonPrintingStyle::CodePoint => NonPrintingLabel::CodePoint,
        }
    }

    /// The settings for `buffer`'s document.
    pub fn for_buffer(buffer: &Buffer, cx: &App) -> Self {
        Self::for_language(&buffer.language_key(), cx)
    }

    /// The column where Split Lines breaks lines: the first edge column while the edge is
    /// shown, as in Notepad++.
    pub fn split_column(&self) -> Option<usize> {
        match self.edge {
            Edge::Off => None,
            Edge::Line | Edge::Background => self.edge_columns.first().copied(),
        }
    }
}
