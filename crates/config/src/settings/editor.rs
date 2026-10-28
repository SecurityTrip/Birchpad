//! Preferences > Editing and Margins, and the delimiters of Select between delimiters.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct EditorSettings {
    pub tab_width: u8,
    pub insert_spaces: bool,
    pub word_wrap: bool,
    /// Margins left of the text, as in Notepad++'s Preferences > Margins: line numbers, the
    /// symbol margin (bookmarks) and the folding margin.
    pub line_numbers: bool,
    pub bookmark_margin: bool,
    pub fold_margin: bool,
    /// Lines changed since the document was opened, as Notepad++'s Change History shows them.
    pub change_history: ChangeHistory,
    /// What Enter does with indentation.
    pub auto_indent: AutoIndent,
    /// View > Show Symbol as Birchpad starts; the menu toggles them (remembered in
    /// `state.toml`, like word wrap).
    pub show_whitespace: bool,
    pub show_eol: bool,
    pub indent_guides: bool,
    pub wrap_symbol: bool,
    /// Spaces that are not plain ones and characters without width, marked.
    pub show_non_printing: bool,
    /// Control characters and Unicode's other line breaks as pictures (␀, ␤); off, as spaces.
    pub show_control_chars: bool,
    /// How Show Non-Printing Characters draws them ("Non-Printing Characters").
    pub non_printing_style: NonPrintingStyle,
    /// How the line of the caret stands out.
    pub current_line: CurrentLine,
    /// Width of the frame around the current line in pixels (1 to 6), as in Notepad++.
    pub current_line_frame_width: u8,
    /// The vertical edge, as in Notepad++'s Preferences > Margins/Border/Edge.
    pub edge: Edge,
    /// Columns of the edge: a line at each in line mode, the first one in background mode.
    /// Split Lines breaks lines at the first one while the edge is shown, else at the width of
    /// the view.
    pub edge_columns: Vec<u16>,
    /// Sticky scroll: the first lines of the folds around the top of the view stay at its top.
    pub sticky_scroll: bool,
    /// The most lines sticky scroll keeps, from 1 to 10.
    pub sticky_scroll_lines: u8,
    /// How the caret is drawn ("Caret Settings > Width").
    pub caret_width: CaretWidth,
    /// Milliseconds between the caret's blinks, from 0 (it does not blink) to 2000.
    pub caret_blink_ms: u16,
    /// Where the rows of a wrapped line start ("Line Wrap").
    pub wrap_indent: WrapIndent,
    /// The caret goes past the end of lines, spaces filling the gap when typing there
    /// ("Enable virtual space").
    pub virtual_space: bool,
    /// The last line can scroll up to the top ("Scrolling beyond last line").
    pub scroll_beyond_last_line: bool,
    /// A right click outside the selection keeps it ("Keep selection when right-click outside
    /// of selection").
    pub keep_selection_on_right_click: bool,
    /// Copy and Cut with nothing selected take the caret's whole line.
    pub copy_line_without_selection: bool,
    /// Selected text in the theme's selection foreground ("Apply selected text foreground
    /// colour").
    pub selection_foreground: bool,
    /// The selected text can be dragged elsewhere, Ctrl copying it ("Disable selected text
    /// drag-and-drop" off).
    pub drag_text: bool,
    /// Ctrl+click adds a caret ("Enable Multi-Editing").
    pub multi_editing: bool,
    /// Typing in a rectangular selection makes it a selection of each row of its own, as
    /// multi-editing goes on ("Enable Column Selection to Multi-editing").
    pub column_to_multi_editing: bool,
    /// How line endings show with View > Show Symbol ("EOL (CRLF) rendering").
    pub eol_style: EolStyle,
    /// Text drawn with subpixel smoothing where the platform has it ("Enable smooth font");
    /// off, as the platform smooths text by default.
    pub smooth_font: bool,
    /// Keys that would type a control character (Ctrl+E) type nothing ("Prevent control
    /// character (C0 code) typing into document").
    pub prevent_control_typing: bool,
    /// Backspace in a line's indentation takes it out a level ("Backspace key unindents
    /// instead of removing single space").
    pub backspace_unindents: bool,
    /// How the folding margin draws fold points ("Fold Margin Style").
    pub fold_style: FoldStyle,
    /// The line between the margins and the text, in pixels from 0 to 30 ("Border Width").
    pub border_width: u8,
    /// The line number margin grows with the number of lines, or keeps room for a million
    /// ("Dynamic width" or "Constant width").
    pub line_number_width: LineNumberWidth,
    /// Room left and right of the text, in pixels from 0 to 30 ("Padding").
    pub padding_left: u8,
    pub padding_right: u8,
    /// How much of the window's width Distraction Free mode leaves on each side of the text,
    /// from 0 (none) to 9 tenths of half of it ("Distraction Free" padding).
    pub padding_distraction_free: u8,
}

/// Notepad++'s fold margin styles.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FoldStyle {
    /// A minus or a plus sign.
    Simple,
    /// An arrow, down when open, right when folded.
    Arrow,
    /// Circles joined by the fold's lines.
    CircleTree,
    /// Boxes joined by the fold's lines.
    #[default]
    BoxTree,
    /// No fold points drawn; clicks still fold.
    None,
}

/// How wide the line number margin is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LineNumberWidth {
    #[default]
    Dynamic,
    Constant,
}

/// Notepad++'s caret widths.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CaretWidth {
    /// No caret at all.
    #[serde(rename = "0")]
    None,
    #[serde(rename = "1")]
    One,
    #[default]
    #[serde(rename = "2")]
    Two,
    #[serde(rename = "3")]
    Three,
    /// A block over the character after the caret.
    Block,
    /// A block over the character before the caret.
    BlockAfter,
}

/// Where the rows after the first of a wrapped line start.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WrapIndent {
    /// At the left edge.
    #[default]
    Default,
    /// Under the line's indentation.
    Aligned,
    /// One indentation step further in than the line.
    Indent,
}

/// How Show Non-Printing Characters draws the characters it shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NonPrintingStyle {
    /// In their own cells: spaces framed, characters without width marked by a line.
    #[default]
    Marks,
    /// Their abbreviation in a box (NBSP, ZWSP), or their code point when they have none.
    Abbreviation,
    /// Their code point in a box (U+00A0).
    CodePoint,
}

/// How line endings show.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EolStyle {
    /// A rounded box labeled CR, LF or CRLF.
    #[default]
    Box,
    /// The label alone.
    PlainText,
}

impl EditorSettings {
    /// [`Self::sticky_scroll_lines`] within its limits.
    pub fn sticky_lines(&self) -> usize {
        usize::from(self.sticky_scroll_lines.clamp(1, 10))
    }
}

/// Where Notepad++'s change history shows changed lines: orange while they differ from the
/// saved file, green once saved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeHistory {
    Off,
    /// A bar in a margin between the symbols and the folding.
    #[default]
    Margin,
}

/// Notepad++'s current line indicator.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CurrentLine {
    Off,
    /// A background across the whole line.
    #[default]
    Background,
    /// A frame around the line.
    Frame,
}

/// Notepad++'s vertical edge modes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Edge {
    #[default]
    Off,
    /// A vertical line at each edge column.
    Line,
    /// Text past the first edge column gets the edge color as its background.
    Background,
}

/// Notepad++'s auto-indent modes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AutoIndent {
    /// A new line starts at column 1.
    Off,
    /// A new line gets the indentation of the line above.
    Basic,
    /// Like basic, and one level more after an opening bracket (or a colon in Python); Enter
    /// between a pair of braces puts the closing one on its own line.
    #[default]
    Advanced,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            tab_width: 4,
            insert_spaces: false,
            word_wrap: false,
            line_numbers: true,
            bookmark_margin: true,
            fold_margin: true,
            change_history: ChangeHistory::default(),
            auto_indent: AutoIndent::default(),
            show_whitespace: false,
            show_eol: false,
            indent_guides: true,
            wrap_symbol: false,
            show_non_printing: false,
            show_control_chars: true,
            non_printing_style: NonPrintingStyle::default(),
            current_line: CurrentLine::default(),
            current_line_frame_width: 1,
            edge: Edge::default(),
            edge_columns: vec![80],
            sticky_scroll: false,
            sticky_scroll_lines: 5,
            caret_width: CaretWidth::default(),
            caret_blink_ms: 530,
            wrap_indent: WrapIndent::default(),
            virtual_space: false,
            scroll_beyond_last_line: false,
            keep_selection_on_right_click: false,
            copy_line_without_selection: false,
            selection_foreground: false,
            drag_text: true,
            multi_editing: true,
            column_to_multi_editing: false,
            eol_style: EolStyle::default(),
            smooth_font: false,
            prevent_control_typing: true,
            backspace_unindents: false,
            fold_style: FoldStyle::default(),
            border_width: 1,
            line_number_width: LineNumberWidth::default(),
            padding_left: 0,
            padding_right: 0,
            padding_distraction_free: 4,
        }
    }
}

/// Ctrl+double click selects between these: Notepad++'s Preferences > Delimiter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct DelimiterSettings {
    /// One character each; anything else turns the selection off.
    pub open: String,
    pub close: String,
    /// The delimiters may be on other lines ("Allow on several lines").
    pub multi_line: bool,
    /// Characters that are part of words besides letters, digits and `_` ("Add your
    /// character as part of word").
    pub word_chars: String,
}

impl Default for DelimiterSettings {
    fn default() -> Self {
        Self {
            open: "\"".to_owned(),
            close: "\"".to_owned(),
            multi_line: false,
            word_chars: String::new(),
        }
    }
}

impl DelimiterSettings {
    /// The opening and the closing character, if both are one character.
    pub fn pair(&self) -> Option<(char, char)> {
        let one = |text: &str| {
            let mut chars = text.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Some(c),
                _ => None,
            }
        };
        Some((one(&self.open)?, one(&self.close)?))
    }
}
