//! Character cells: how many columns of a monospace grid a character takes, and what is drawn in
//! place of characters that have no glyph of their own (tabs, control characters, the labels of
//! non-printing characters).

use std::borrow::Cow;
use std::ops::Range;

use birchpad_core::Rope;
use unicode_width::UnicodeWidthChar;

/// Width of `ch` in cells, for any character but a tab: 2 for East Asian wide characters and
/// emoji, 0 for combining marks, 1 otherwise (including control characters, drawn as a picture).
pub fn char_cells(ch: char) -> usize {
    if (' '..'\u{7F}').contains(&ch) {
        // Printable ASCII, by far the most common case.
        1
    } else if ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}') {
        1
    } else {
        ch.width().unwrap_or(1)
    }
}

/// Columns a tab advances when it starts at `column`.
pub fn tab_advance(column: usize, tab_width: usize) -> usize {
    let tab_width = tab_width.max(1);
    tab_width - column % tab_width
}

/// How Show Non-Printing Characters draws the characters it shows (Preferences > Editing 2 >
/// Non-Printing Characters).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum NonPrintingLabel {
    /// In their own cells: spaces framed, characters without width marked by a line.
    #[default]
    Off,
    /// By their abbreviation (NBSP, ZWSP), or their code point when they have none, in cells of
    /// its own.
    Abbreviation,
    /// By their code point (U+00A0), in cells of its own.
    CodePoint,
}

/// The monospace grid text is laid out on: its tab stops, and the labels whose cells
/// non-printing characters take while they are shown by label. A tab width alone is a grid
/// without labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Grid {
    pub tab_width: usize,
    pub labels: NonPrintingLabel,
}

impl From<usize> for Grid {
    fn from(tab_width: usize) -> Self {
        Self {
            tab_width,
            labels: NonPrintingLabel::Off,
        }
    }
}

impl Grid {
    /// Whether no character takes more cells than its bytes in UTF-8, tabs aside: then a line no
    /// longer in bytes than a width, without tabs, fits in it.
    pub fn cells_within_bytes(self) -> bool {
        self.labels == NonPrintingLabel::Off
    }
}

/// Cells taken by `ch` starting at `column` (tabs depend on the column) on `grid`, a tab width
/// or a [`Grid`].
pub fn cells_at(ch: char, column: usize, grid: impl Into<Grid>) -> usize {
    let grid = grid.into();
    if ch == '\t' {
        tab_advance(column, grid.tab_width)
    } else if grid.labels != NonPrintingLabel::Off && non_printing(ch).is_some() {
        label_len(ch, grid.labels)
    } else {
        char_cells(ch)
    }
}

/// What is drawn for a control character: its Unicode control picture (␀, ␛, ...); for the
/// line breaks of Unicode that Birchpad's lines do not end at (U+0085, U+2028, U+2029), ␤
/// and ¶.
pub fn control_picture(ch: char) -> Option<char> {
    match ch {
        '\t' => None,
        '\0'..='\u{1F}' => char::from_u32(0x2400 + ch as u32),
        '\u{7F}' => Some('\u{2421}'),
        '\u{85}' | '\u{2028}' => Some('\u{2424}'),
        '\u{80}'..='\u{9F}' => Some('\u{25AF}'),
        '\u{2029}' => Some('\u{B6}'),
        _ => None,
    }
}

/// A character that shows nothing of itself, for View > Show Symbol > Show Non-Printing
/// Characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NonPrinting {
    /// A space that is not a plain one (no-break, en, em, thin, ideographic ...): its cells
    /// are framed.
    Space,
    /// A character without width (zero-width space and joiners, direction marks, the byte
    /// order mark, the soft hyphen): a line marks where it is.
    Mark,
}

/// Whether `ch` is a character [`NonPrinting`] shows.
pub fn non_printing(ch: char) -> Option<NonPrinting> {
    match ch {
        '\u{A0}' | '\u{1680}' | '\u{2000}'..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}' => {
            Some(NonPrinting::Space)
        }
        '\u{AD}'
        | '\u{180E}'
        | '\u{200B}'..='\u{200F}'
        | '\u{202A}'..='\u{202E}'
        | '\u{2060}'..='\u{2064}'
        | '\u{2066}'..='\u{2069}'
        | '\u{FEFF}' => Some(NonPrinting::Mark),
        _ => None,
    }
}

/// The abbreviation of a non-printing character: Unicode's for those it gives one, the short
/// form its code charts print for the other spaces; none for the rest.
pub fn abbreviation(ch: char) -> Option<&'static str> {
    Some(match ch {
        '\u{A0}' => "NBSP",
        '\u{AD}' => "SHY",
        '\u{180E}' => "MVS",
        '\u{2000}' => "NQSP",
        '\u{2001}' => "MQSP",
        '\u{2002}' => "ENSP",
        '\u{2003}' => "EMSP",
        '\u{2004}' => "3/MSP",
        '\u{2005}' => "4/MSP",
        '\u{2006}' => "6/MSP",
        '\u{2007}' => "FSP",
        '\u{2008}' => "PSP",
        '\u{2009}' => "THSP",
        '\u{200A}' => "HSP",
        '\u{200B}' => "ZWSP",
        '\u{200C}' => "ZWNJ",
        '\u{200D}' => "ZWJ",
        '\u{200E}' => "LRM",
        '\u{200F}' => "RLM",
        '\u{202A}' => "LRE",
        '\u{202B}' => "RLE",
        '\u{202C}' => "PDF",
        '\u{202D}' => "LRO",
        '\u{202E}' => "RLO",
        '\u{202F}' => "NNBSP",
        '\u{205F}' => "MMSP",
        '\u{2060}' => "WJ",
        '\u{2066}' => "LRI",
        '\u{2067}' => "RLI",
        '\u{2068}' => "FSI",
        '\u{2069}' => "PDI",
        '\u{3000}' => "IDSP",
        '\u{FEFF}' => "BOM",
        _ => return None,
    })
}

/// The label `labels` draws for `ch`, if it is a character [`NonPrinting`] shows.
pub fn non_printing_label(ch: char, labels: NonPrintingLabel) -> Option<Cow<'static, str>> {
    non_printing(ch)?;
    match (labels, abbreviation(ch)) {
        (NonPrintingLabel::Off, _) => None,
        (NonPrintingLabel::Abbreviation, Some(short)) => Some(Cow::Borrowed(short)),
        _ => Some(Cow::Owned(format!("U+{:04X}", ch as u32))),
    }
}

/// Cells of the label of `ch`, a character [`NonPrinting`] shows, without building it: its
/// characters are all ASCII.
fn label_len(ch: char, labels: NonPrintingLabel) -> usize {
    match (labels, abbreviation(ch)) {
        (NonPrintingLabel::Abbreviation, Some(short)) => short.len(),
        // "U+" and the hex digits, at least four.
        _ => 2 + (ch as u32).max(0x1000).ilog(16) as usize + 1,
    }
}

/// The column at which `pos` starts, scanning from `start` (at column `start_column`), on
/// `grid`.
pub fn column_after(
    text: &Rope,
    start: usize,
    start_column: usize,
    pos: usize,
    grid: impl Into<Grid>,
) -> usize {
    let grid = grid.into();
    let mut column = start_column;
    for ch in text.slice(start..pos).chars() {
        column += cells_at(ch, column, grid);
    }
    column
}

/// The character boundary in `range` closest to `target` columns, scanning from `range.start`
/// at `start_column`, on `grid`. A position in the right half of a character rounds up.
pub fn pos_at_column(
    text: &Rope,
    range: Range<usize>,
    start_column: usize,
    target: usize,
    grid: impl Into<Grid>,
) -> (usize, usize) {
    let grid = grid.into();
    let mut column = start_column;
    let mut pos = range.start;
    for ch in text.slice(range.clone()).chars() {
        if column >= target {
            break;
        }
        let cells = cells_at(ch, column, grid);
        // Past the middle of the character: the caret goes after it.
        if column + cells > target && (target - column) * 2 < cells {
            break;
        }
        column += cells;
        pos += ch.len_utf8();
    }
    (pos, column)
}

/// Text to draw for a range of the document, with tabs expanded to spaces, control characters
/// replaced by pictures and non-printing characters by their labels while the grid has them,
/// and the mapping between document and display offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayText {
    pub text: String,
    doc_start: usize,
    /// Substituted characters, in order: document range and display range.
    substitutions: Vec<(Range<usize>, Range<usize>)>,
}

impl DisplayText {
    /// Builds the display text of `range`, which starts at column `start_column` of its line.
    pub fn new(
        text: &Rope,
        range: Range<usize>,
        start_column: usize,
        grid: impl Into<Grid>,
    ) -> Self {
        Self::with_pictures(text, range, start_column, grid, true)
    }

    /// As [`Self::new`]; without `pictures` (View > Show Symbol > Show Control Characters &
    /// Unicode EOL turned off) control characters show as spaces.
    pub fn with_pictures(
        text: &Rope,
        range: Range<usize>,
        start_column: usize,
        grid: impl Into<Grid>,
        pictures: bool,
    ) -> Self {
        let grid = grid.into();
        let mut out = String::with_capacity(range.len());
        let mut substitutions = Vec::new();
        let mut column = start_column;
        let mut pos = range.start;
        for ch in text.slice(range.clone()).chars() {
            let cells = cells_at(ch, column, grid);
            let display_start = out.len();
            let replaced = if ch == '\t' {
                out.extend(std::iter::repeat_n(' ', cells));
                true
            } else if let Some(picture) = control_picture(ch) {
                out.push(if pictures { picture } else { ' ' });
                true
            } else if let Some(label) = non_printing_label(ch, grid.labels) {
                out.push_str(&label);
                true
            } else {
                out.push(ch);
                false
            };
            // Even a replacement as long as the character: an offset inside it is none of the
            // document's.
            if replaced {
                substitutions.push((pos..pos + ch.len_utf8(), display_start..out.len()));
            }
            column += cells;
            pos += ch.len_utf8();
        }
        Self {
            text: out,
            doc_start: range.start,
            substitutions,
        }
    }

    pub fn doc_start(&self) -> usize {
        self.doc_start
    }

    pub fn doc_end(&self) -> usize {
        self.to_doc(self.text.len())
    }

    /// Display offset of a document position inside the range.
    pub fn to_display(&self, pos: usize) -> usize {
        let mut delta: isize = 0;
        for (doc, display) in &self.substitutions {
            if pos <= doc.start {
                break;
            }
            if pos < doc.end {
                return display.start;
            }
            delta += display.len() as isize - doc.len() as isize;
        }
        ((pos - self.doc_start) as isize + delta) as usize
    }

    /// Document position of a display offset; offsets inside an expanded tab or a picture snap
    /// to the nearer end of the character.
    pub fn to_doc(&self, offset: usize) -> usize {
        let mut delta: isize = 0;
        for (doc, display) in &self.substitutions {
            if offset < display.start {
                break;
            }
            if offset < display.end {
                let into = offset - display.start;
                return if into * 2 < display.len() {
                    doc.start
                } else {
                    doc.end
                };
            }
            delta += display.len() as isize - doc.len() as isize;
        }
        (self.doc_start as isize + offset as isize - delta) as usize
    }

    /// The characters of the range in order: document position, character and display range.
    /// One pass, unlike calling [`Self::to_display`] for each.
    pub fn chars<'a>(
        &'a self,
        text: &'a Rope,
    ) -> impl Iterator<Item = (usize, char, Range<usize>)> + 'a {
        let mut substitutions = self.substitutions.iter().peekable();
        let mut pos = self.doc_start;
        let mut offset = 0;
        text.slice(self.doc_start..self.doc_end())
            .chars()
            .map(move |ch| {
                let start = pos;
                pos += ch.len_utf8();
                let display = match substitutions.next_if(|(doc, _)| doc.start == start) {
                    Some((_, display)) => display.clone(),
                    None => offset..offset + ch.len_utf8(),
                };
                offset = display.end;
                (start, ch, display)
            })
    }
}
