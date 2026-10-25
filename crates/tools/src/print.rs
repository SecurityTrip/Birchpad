//! Printing (ADR 0044): a document laid out on pages of paper, with a header and a footer made
//! of Notepad++'s variables, line numbers and the colors chosen. The print preview, the pages
//! printed through the system and the page printed through a browser all draw these pages, so
//! that what the preview shows is what comes out.
//!
//! Sizes are in millimeters and points, not in a device's dots: the text is monospaced, a cell
//! [`CELL_WIDTH`] of the font's size wide and a row [`ROW_HEIGHT`] of it high, so that a page
//! holds as many cells and rows on every printer.

use std::fmt::Write as _;
use std::ops::Range;

use birchpad_theme::{Color, EditorColors, Style};

/// A cell's width, as a share of the font's size (a monospaced font's advance).
pub const CELL_WIDTH: f32 = 0.6;
/// A row's height, as a share of the font's size.
pub const ROW_HEIGHT: f32 = 1.2;
/// Millimeters in a point.
pub const MM_PER_POINT: f32 = 25.4 / 72.0;

/// The paper printed on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Paper {
    A3,
    #[default]
    A4,
    A5,
    Letter,
    Legal,
}

impl Paper {
    /// Width and height in millimeters, upright.
    pub fn size(self) -> (f32, f32) {
        match self {
            Self::A3 => (297.0, 420.0),
            Self::A4 => (210.0, 297.0),
            Self::A5 => (148.0, 210.0),
            Self::Letter => (215.9, 279.4),
            Self::Legal => (215.9, 355.6),
        }
    }
}

/// The colors printed (Notepad++'s Colour Options).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PrintColors {
    /// As on screen, the backgrounds too.
    #[default]
    Wysiwyg,
    /// The screen's colors inverted.
    Invert,
    /// Black text on white paper; bold, italic and underline kept.
    BlackOnWhite,
    /// The text's colors on white paper, without backgrounds.
    NoBackground,
}

/// Margins from the paper's edges, in millimeters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Margins {
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
}

/// How the pages look.
#[derive(Debug, Clone, PartialEq)]
pub struct PageSetup {
    pub paper: Paper,
    pub landscape: bool,
    pub margins: Margins,
    /// The text's size in points.
    pub font_size: f32,
    pub line_numbers: bool,
    /// Left, center and right parts, with variables (see [`expand`]).
    pub header: [String; 3],
    pub footer: [String; 3],
    pub colors: PrintColors,
    pub tab_width: usize,
    /// A form feed (U+000C) ends its page: the line after it starts the next one.
    pub form_feed: bool,
}

impl Default for PageSetup {
    fn default() -> Self {
        Self {
            paper: Paper::A4,
            landscape: false,
            margins: Margins {
                top: 15.0,
                bottom: 15.0,
                left: 15.0,
                right: 15.0,
            },
            font_size: 10.0,
            line_numbers: true,
            header: Default::default(),
            footer: Default::default(),
            colors: PrintColors::Wysiwyg,
            tab_width: 4,
            form_feed: false,
        }
    }
}

impl PageSetup {
    /// The page's width and height in millimeters, turned for landscape.
    pub fn page_size(&self) -> (f32, f32) {
        let (width, height) = self.paper.size();
        if self.landscape {
            (height, width)
        } else {
            (width, height)
        }
    }

    /// A cell's width and a row's height in millimeters.
    pub fn cell(&self) -> (f32, f32) {
        let size = self.font_size.max(1.0) * MM_PER_POINT;
        (size * CELL_WIDTH, size * ROW_HEIGHT)
    }

    /// Cells across the text, line numbers included, and rows down it, header and footer
    /// included: at least one of each, however large the margins.
    pub fn grid(&self) -> (usize, usize) {
        let (width, height) = self.page_size();
        let (cell, row) = self.cell();
        let across = (width - self.margins.left - self.margins.right) / cell;
        let down = (height - self.margins.top - self.margins.bottom) / row;
        (
            (across.floor() as usize).max(1),
            (down.floor() as usize).max(1),
        )
    }

    /// Whether the header, or the footer, has something to print.
    fn has(parts: &[String; 3]) -> bool {
        parts.iter().any(|part| !part.trim().is_empty())
    }
}

/// What Notepad++'s header and footer variables stand for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Variables {
    /// `$(FULL_CURRENT_PATH)`: the file's whole path, or the document's name if it has no file.
    pub full_path: String,
    /// `$(FILE_NAME)`.
    pub file_name: String,
    /// `$(CURRENT_DIRECTORY)`: the folder of the file.
    pub directory: String,
    /// `$(SHORT_DATE)`, `$(LONG_DATE)` and `$(TIME)`, written as the user's settings say.
    pub short_date: String,
    pub long_date: String,
    pub time: String,
}

/// `template` with its variables filled in for page `page` (from 1). An unknown variable, or
/// a `$(` without its `)`, stays as written.
pub fn expand(template: &str, variables: &Variables, page: usize) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("$(") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find(')') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let value = match &after[..end] {
            "FULL_CURRENT_PATH" => Some(variables.full_path.clone()),
            "FILE_NAME" => Some(variables.file_name.clone()),
            "CURRENT_DIRECTORY" => Some(variables.directory.clone()),
            "CURRENT_PRINTING_PAGE" => Some(page.to_string()),
            "SHORT_DATE" => Some(variables.short_date.clone()),
            "LONG_DATE" => Some(variables.long_date.clone()),
            "TIME" => Some(variables.time.clone()),
            _ => None,
        };
        match value {
            Some(value) => out.push_str(&value),
            None => out.push_str(&rest[start..start + 2 + end + 1]),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// A run of text in one style, tabs already turned into spaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub text: String,
    /// The cell it starts at, after the line number.
    pub column: usize,
    pub style: Style,
}

/// A row of a page: a line, or the part of a long line that did not fit on the row above.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Row {
    /// The line's number on its first row; nothing on the rows it wraps onto.
    pub number: Option<usize>,
    pub pieces: Vec<Piece>,
}

/// A page, ready to draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// From 1.
    pub number: usize,
    /// The header's and footer's parts with their variables filled in; all empty when there is
    /// no header or footer.
    pub header: [String; 3],
    pub footer: [String; 3],
    pub rows: Vec<Row>,
}

/// The pages of a document, as [`paginate`] lays them out, with what they share.
#[derive(Debug, Clone, PartialEq)]
pub struct Pages {
    pub pages: Vec<Page>,
    /// Cells taken by the line numbers and the space after them; 0 without line numbers.
    pub number_width: usize,
    /// The paper's color, and the line numbers' and the header's.
    pub background: Color,
    pub numbers: Color,
    pub text: Color,
}

/// What to print: `text`, whose first line is line `first_line` (from 1) of its document, and
/// the styles of its ranges (byte offsets into `text`; where they nest, the later one wins).
#[derive(Debug, Clone, Copy)]
pub struct Document<'a> {
    pub text: &'a str,
    pub first_line: usize,
    pub spans: &'a [(Range<usize>, Style)],
}

/// The color `color` printed as `colors` says, over the editor's `colors`.
fn print_color(color: Color, mode: PrintColors) -> Color {
    match mode {
        PrintColors::Invert => {
            let [r, g, b, a] = color.0.to_be_bytes();
            Color(u32::from_be_bytes([255 - r, 255 - g, 255 - b, a]))
        }
        _ => color,
    }
}

/// The style a piece is printed in: every color explicit.
pub fn print_style(style: Style, editor: &EditorColors, mode: PrintColors) -> Style {
    let color = style.color.unwrap_or(editor.text);
    match mode {
        PrintColors::Wysiwyg | PrintColors::Invert => Style {
            color: Some(print_color(color, mode)),
            background: style
                .background
                .map(|background| print_color(background, mode)),
            ..style
        },
        PrintColors::BlackOnWhite => Style {
            color: Some(Color::rgb(0)),
            background: None,
            ..style
        },
        PrintColors::NoBackground => Style {
            color: Some(color),
            background: None,
            ..style
        },
    }
}

/// The runs of one style in `start..end` of the text, from `flat` (the document's styles as
/// [`crate::html::flatten`] gives them: pieces in order, covering the text).
fn runs(
    flat: &[(Range<usize>, Option<usize>)],
    spans: &[(Range<usize>, Style)],
    start: usize,
    end: usize,
) -> Vec<(Range<usize>, Style)> {
    let first = flat.partition_point(|(range, _)| range.end <= start);
    flat[first..]
        .iter()
        .take_while(|(range, _)| range.start < end)
        .map(|(range, style)| {
            let style = style.map_or_else(Style::default, |index| spans[index].1);
            (range.start.max(start)..range.end.min(end), style)
        })
        .filter(|(range, _)| !range.is_empty())
        .collect()
}

/// Cells `ch` takes at `column`: tabs to the next stop, East Asian wide characters two.
fn cells(ch: char, column: usize, tab_width: usize) -> usize {
    if ch == '\t' {
        let tab_width = tab_width.max(1);
        tab_width - column % tab_width
    } else if is_wide(ch) {
        2
    } else {
        1
    }
}

/// The wide ranges of East Asian scripts (CJK, Hangul, full-width forms) and emoji.
fn is_wide(ch: char) -> bool {
    matches!(u32::from(ch),
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F
        | 0x1F900..=0x1F9FF
        | 0x20000..=0x3FFFD)
}

/// The rows of the line `start..end` of `text` (without its line break) in its `runs`, wrapped
/// at `columns` cells.
fn line_rows(
    text: &str,
    runs: Vec<(Range<usize>, Style)>,
    number: usize,
    columns: usize,
    tab_width: usize,
    style_of: &impl Fn(Style) -> Style,
) -> Vec<Row> {
    let mut rows = vec![Row {
        number: Some(number),
        pieces: Vec::new(),
    }];
    // The column on the line (for tab stops) and on the row.
    let mut line_column = 0;
    let mut column = 0;
    for (range, style) in runs {
        let style = style_of(style);
        let mut piece = Piece {
            text: String::new(),
            column,
            style,
        };
        for ch in text[range].chars() {
            let width = cells(ch, line_column, tab_width);
            if column + width > columns && column > 0 {
                let full = Piece {
                    text: std::mem::take(&mut piece.text),
                    column: piece.column,
                    style,
                };
                let row = rows.last_mut().expect("one row at least");
                if !full.text.is_empty() {
                    row.pieces.push(full);
                }
                rows.push(Row::default());
                column = 0;
                piece.column = 0;
            }
            if ch == '\t' {
                piece.text.extend(std::iter::repeat_n(' ', width));
            } else if ch.is_control() {
                piece.text.push(' ');
            } else {
                piece.text.push(ch);
            }
            column += width;
            line_column += width;
        }
        if !piece.text.is_empty() {
            rows.last_mut()
                .expect("one row at least")
                .pieces
                .push(piece);
        }
    }
    rows
}

/// Lays `document` out on pages as `setup` says, its header and footer filled in from
/// `variables`, its colors from `editor` (the theme's). An empty document is one empty page.
pub fn paginate(
    document: &Document<'_>,
    setup: &PageSetup,
    variables: &Variables,
    editor: &EditorColors,
) -> Pages {
    let (across, down) = setup.grid();
    let header = PageSetup::has(&setup.header);
    let footer = PageSetup::has(&setup.footer);
    // A header or footer takes its row and an empty one between it and the text.
    let reserved = 2 * usize::from(header) + 2 * usize::from(footer);
    let rows_per_page = down.saturating_sub(reserved).max(1);

    let lines: Vec<(usize, usize)> = {
        let mut lines = Vec::new();
        let mut start = 0;
        let text = document.text;
        for (at, _) in text.match_indices('\n') {
            let end = if at > start && text.as_bytes()[at - 1] == b'\r' {
                at - 1
            } else {
                at
            };
            lines.push((start, end));
            start = at + 1;
        }
        let end = if text.ends_with('\r') {
            text.len() - 1
        } else {
            text.len()
        };
        if start < text.len() || lines.is_empty() {
            lines.push((start, end.max(start)));
        }
        lines
    };
    let last_number = document.first_line + lines.len().saturating_sub(1);
    let number_width = if setup.line_numbers {
        last_number.to_string().len() + 1
    } else {
        0
    };
    let columns = across.saturating_sub(number_width).max(1);
    let style_of = |style: Style| print_style(style, editor, setup.colors);
    let flat = crate::html::flatten(document.spans, document.text.len());

    // The rows, and the pages they make: full ones, or ended after a form feed's line.
    let mut groups: Vec<Vec<Row>> = vec![Vec::new()];
    for (index, (start, end)) in lines.into_iter().enumerate() {
        let feeds = setup.form_feed && document.text[start..end].contains('\u{c}');
        for row in line_rows(
            document.text,
            runs(&flat, document.spans, start, end),
            document.first_line + index,
            columns,
            setup.tab_width,
            &style_of,
        ) {
            if groups
                .last()
                .is_some_and(|page| page.len() >= rows_per_page)
            {
                groups.push(Vec::new());
            }
            groups.last_mut().expect("one page at least").push(row);
        }
        if feeds {
            groups.push(Vec::new());
        }
    }
    if groups.len() > 1 && groups.last().is_some_and(Vec::is_empty) {
        groups.pop();
    }
    let pages = groups
        .iter()
        .enumerate()
        .map(|(index, rows)| {
            let number = index + 1;
            let fill = |parts: &[String; 3], shown: bool| {
                if shown {
                    parts
                        .clone()
                        .map(|part| expand(part.trim(), variables, number))
                } else {
                    Default::default()
                }
            };
            Page {
                number,
                header: fill(&setup.header, header),
                footer: fill(&setup.footer, footer),
                rows: rows.to_vec(),
            }
        })
        .collect();
    let (background, numbers, text) = match setup.colors {
        PrintColors::Wysiwyg | PrintColors::Invert => (
            print_color(editor.background, setup.colors),
            print_color(editor.gutter_text, setup.colors),
            print_color(editor.text, setup.colors),
        ),
        PrintColors::BlackOnWhite | PrintColors::NoBackground => {
            (Color::rgb(0xffffff), Color::rgb(0), Color::rgb(0))
        }
    };
    Pages {
        pages,
        number_width,
        background,
        numbers,
        text,
    }
}

fn css_color(color: Color) -> String {
    let [r, g, b, a] = color.0.to_be_bytes();
    if a == 0xff {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("rgba({r},{g},{b},{:.3})", f32::from(a) / 255.)
    }
}

fn escape(text: &str, out: &mut String) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
}

/// The pages as one HTML document of pages of the paper's size, each laid out as [`paginate`]
/// made it, for a browser to print (`print` asks the browser to, once loaded).
pub fn to_html(title: &str, pages: &Pages, setup: &PageSetup, print: bool) -> String {
    let (width, height) = setup.page_size();
    let (cell, row) = setup.cell();
    let margins = setup.margins;
    let mut html = String::with_capacity(4096);
    html.push_str("<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>");
    escape(title, &mut html);
    let _ = write!(
        html,
        "</title><style>\
         @page{{size:{width}mm {height}mm;margin:0}}\
         html,body{{margin:0;padding:0}}\
         .page{{box-sizing:border-box;width:{width}mm;height:{height}mm;overflow:hidden;\
         position:relative;page-break-after:always;break-after:page;\
         padding:{top}mm {right}mm {bottom}mm {left}mm;background:{background};\
         color:{text};-webkit-print-color-adjust:exact;print-color-adjust:exact;\
         font:{size}pt Consolas,'DejaVu Sans Mono',Menlo,monospace;line-height:{row}mm}}\
         .row{{height:{row}mm;white-space:pre;position:relative}}\
         .row span{{position:absolute;top:0}}\
         .n{{color:{numbers}}}\
         .hf{{height:{row}mm;display:flex;justify-content:space-between;white-space:pre}}\
         .hf span{{flex:1;overflow:hidden}}.hf .c{{text-align:center}}.hf .r{{text-align:right}}\
         .gap{{height:{row}mm}}\
         .footer{{position:absolute;left:{left}mm;right:{right}mm;bottom:{bottom}mm}}\
         </style>",
        top = margins.top,
        right = margins.right,
        bottom = margins.bottom,
        left = margins.left,
        background = css_color(pages.background),
        text = css_color(pages.text),
        numbers = css_color(pages.numbers),
        size = setup.font_size,
    );
    if print {
        html.push_str("<script>window.addEventListener('load',()=>window.print())</script>");
    }
    html.push_str("</head><body>");
    let parts = |html: &mut String, parts: &[String; 3]| {
        for (part, class) in parts.iter().zip(["l", "c", "r"]) {
            let _ = write!(html, "<span class=\"{class}\">");
            escape(part, html);
            html.push_str("</span>");
        }
    };
    let has = |parts: &[String; 3]| parts.iter().any(|part| !part.is_empty());
    for page in &pages.pages {
        html.push_str("<div class=\"page\">");
        if has(&page.header) {
            html.push_str("<div class=\"hf\">");
            parts(&mut html, &page.header);
            html.push_str("</div><div class=\"gap\"></div>");
        }
        for line in &page.rows {
            html.push_str("<div class=\"row\">");
            if let Some(number) = line.number.filter(|_| pages.number_width > 0) {
                let _ = write!(
                    html,
                    "<span class=\"n\" style=\"left:0\">{number:>width$}</span>",
                    width = pages.number_width - 1
                );
            }
            for piece in &line.pieces {
                let left = (pages.number_width + piece.column) as f32 * cell;
                let _ = write!(html, "<span style=\"left:{left:.3}mm;");
                let style = piece.style;
                if let Some(color) = style.color {
                    let _ = write!(html, "color:{};", css_color(color));
                }
                if let Some(background) = style.background {
                    let _ = write!(html, "background:{};", css_color(background));
                }
                if style.bold {
                    html.push_str("font-weight:bold;");
                }
                if style.italic {
                    html.push_str("font-style:italic;");
                }
                if style.underline {
                    html.push_str("text-decoration:underline;");
                }
                html.push_str("\">");
                escape(&piece.text, &mut html);
                html.push_str("</span>");
            }
            html.push_str("</div>");
        }
        if has(&page.footer) {
            html.push_str("<div class=\"footer hf\">");
            parts(&mut html, &page.footer);
            html.push_str("</div>");
        }
        html.push_str("</div>");
    }
    html.push_str("</body></html>\n");
    html
}
