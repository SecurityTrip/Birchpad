//! What the editor draws besides the text to show its structure: View > Show Symbol (spaces,
//! tabs, line endings, indentation guides, wrap symbols), the vertical edge and the current line,
//! as Notepad++ draws them.

use birchpad_config::{CurrentLine, Edge};
use birchpad_core::motion::line_of;
use birchpad_core::{LineEnding, Rope};
use gpui_kit::{Bounds, Hsla, Pixels, ShapedLine, TextRun, Window, point, px, size};

use super::layout::{Layout, VisibleRow};
use super::{EditorView, ViewSettings, theme};

/// Positions of the visual aids in one frame.
#[derive(Default)]
pub struct Symbols {
    /// The rows of the primary caret's line, across the text area.
    pub current_line: Option<Bounds<Pixels>>,
    /// Draw the current line as a frame of this width rather than a background.
    pub current_line_frame: Option<Pixels>,
    /// Vertical lines at the edge columns, across the text area.
    pub(crate) edge_lines: Vec<Bounds<Pixels>>,
    /// Text past the edge column, in background mode.
    pub(crate) edge_backgrounds: Vec<Bounds<Pixels>>,
    /// Indentation guides, one pixel wide, drawn dotted.
    pub(crate) indent_guides: Vec<Bounds<Pixels>>,
    /// A dot for each space.
    pub(crate) spaces: Vec<Bounds<Pixels>>,
    /// The cells of each tab, drawn as an arrow across them.
    pub(crate) tabs: Vec<Bounds<Pixels>>,
    /// Boxes labeled CR, LF or CRLF after the end of lines.
    pub(crate) line_breaks: Vec<(Bounds<Pixels>, ShapedLine)>,
    /// The cells where wrapped rows show the wrap symbol.
    pub(crate) wrap_marks: Vec<Bounds<Pixels>>,
}

impl EditorView {
    /// Computes the visual aids for the rows of `layout`.
    pub(crate) fn layout_symbols(
        &mut self,
        layout: &mut Layout,
        text: &Rope,
        settings: &ViewSettings,
        window: &mut Window,
    ) {
        let mut symbols = Symbols::default();
        if settings.current_line != CurrentLine::Off {
            symbols.current_line = self.current_line_bounds(layout, text);
            symbols.current_line_frame = (settings.current_line == CurrentLine::Frame)
                .then_some(settings.current_line_frame_width);
        }
        let cell = layout.metrics.cell;
        match settings.edge {
            Edge::Off => {}
            Edge::Line => {
                let area = layout.text_bounds;
                symbols.edge_lines = settings
                    .edge_columns
                    .iter()
                    .map(|&column| {
                        let x = (layout.column_zero + cell * column as f32).round();
                        Bounds::new(point(x, area.top()), size(px(1.), area.size.height))
                    })
                    .collect();
            }
            Edge::Background => {
                if let Some(&column) = settings.edge_columns.first() {
                    symbols.edge_backgrounds = self.edge_backgrounds(layout, text, column);
                }
            }
        }
        if settings.indent_guides {
            symbols.indent_guides = self.indent_guides(layout, text, settings.tab_width);
        }
        if settings.show_whitespace {
            whitespace_marks(layout, text, &mut symbols);
        }
        if settings.show_eol {
            symbols.line_breaks = self.line_break_marks(layout, text, window);
        }
        if settings.wrap_symbol
            && let Some(wrap_width) = self.display.config().wrap_width
        {
            let x = layout.column_zero + cell * wrap_width as f32;
            symbols.wrap_marks = layout
                .rows
                .iter()
                .filter(|row| !row.row.last_in_line)
                .map(|row| Bounds::new(point(x, row.y), size(cell, layout.metrics.line_height)))
                .collect();
        }
        layout.symbols = symbols;
    }

    fn current_line_bounds(&self, layout: &Layout, text: &Rope) -> Option<Bounds<Pixels>> {
        let line = line_of(text, self.selection.primary().head);
        let mut rows = layout.rows.iter().filter(|row| row.row.line == line);
        let first = rows.next()?;
        let last = rows.next_back().unwrap_or(first);
        let area = layout.text_bounds;
        Some(Bounds::from_corners(
            point(area.left(), first.y),
            point(area.right(), last.y + layout.metrics.line_height),
        ))
    }

    /// The text at or past `column` of its line in each visible row.
    fn edge_backgrounds(
        &mut self,
        layout: &Layout,
        text: &Rope,
        column: usize,
    ) -> Vec<Bounds<Pixels>> {
        let mut quads = Vec::new();
        for row in &layout.rows {
            let start = if row.row.start_column >= column {
                row.row.range.start
            } else {
                self.display.pos_at_column(text, &row.row, column)
            };
            if start >= row.row.range.end {
                continue;
            }
            let left = self.x_in_row(layout, row, start, text);
            let right = self.x_in_row(layout, row, row.row.range.end, text);
            quads.push(Bounds::from_corners(
                point(left, row.y),
                point(right, row.y + layout.metrics.line_height),
            ));
        }
        quads
    }

    /// Guides of the first row of each visible line, joined into runs down the rows.
    fn indent_guides(&self, layout: &Layout, text: &Rope, tab_width: usize) -> Vec<Bounds<Pixels>> {
        let folds = self
            .fold_cache
            .as_ref()
            .map_or(&[][..], |cache| cache.folds.as_slice());
        let is_header = |line: usize| birchpad_view::fold_at(folds, line).is_some();
        let line_height = layout.metrics.line_height;
        let mut guides: Vec<Bounds<Pixels>> = Vec::new();
        for row in layout.rows.iter().filter(|row| row.row.index_in_line == 0) {
            for column in birchpad_view::indent_guides(text, row.row.line, tab_width, is_header) {
                let x = (layout.column_zero + layout.metrics.cell * column as f32).round();
                guides.push(Bounds::new(point(x, row.y), size(px(1.), line_height)));
            }
        }
        guides.sort_by(|a, b| {
            (a.left(), a.top())
                .partial_cmp(&(b.left(), b.top()))
                .unwrap()
        });
        let mut runs: Vec<Bounds<Pixels>> = Vec::new();
        for guide in guides {
            match runs.last_mut() {
                Some(run) if run.left() == guide.left() && run.bottom() == guide.top() => {
                    run.size.height += guide.size.height;
                }
                _ => runs.push(guide),
            }
        }
        runs
    }

    fn line_break_marks(
        &mut self,
        layout: &Layout,
        text: &Rope,
        window: &mut Window,
    ) -> Vec<(Bounds<Pixels>, ShapedLine)> {
        let metrics = &layout.metrics;
        let font_size = (metrics.font_size * 0.7).round();
        let height = (font_size * 1.3).round().min(metrics.line_height);
        let padding = (font_size * 0.25).round();
        let color: Hsla = theme::paint(theme::editor().eol_text).into();
        let mut labels: [Option<ShapedLine>; 3] = Default::default();
        let mut marks = Vec::new();
        for row in layout.rows.iter().filter(|row| row.row.last_in_line) {
            let Some(ending) = LineEnding::at(text, row.row.range.end) else {
                continue;
            };
            let label = labels[ending as usize]
                .get_or_insert_with(|| {
                    let label = ending.label();
                    let run = TextRun {
                        len: label.len(),
                        font: metrics.font.clone(),
                        color,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    window
                        .text_system()
                        .shape_line(label.into(), font_size, &[run], None)
                })
                .clone();
            let x = self.x_in_row(layout, row, row.row.range.end, text) + padding;
            let y = row.y + (metrics.line_height - height) / 2.;
            let bounds = Bounds::new(point(x, y), size(label.width() + padding * 2., height));
            marks.push((bounds, label));
        }
        marks
    }
}

/// Dots for spaces and arrows for tabs in the shaped part of each visible row.
fn whitespace_marks(layout: &Layout, text: &Rope, symbols: &mut Symbols) {
    let metrics = &layout.metrics;
    let dot = (metrics.font_size / 7.).round().max(px(1.));
    for row in &layout.rows {
        let mut glyphs = GlyphCursor::new(row);
        for (_, ch, display) in row.display.chars(text) {
            if ch != ' ' && ch != '\t' {
                continue;
            }
            let left = glyphs.x_at(display.start);
            let right = glyphs.x_at(display.end);
            if ch == ' ' {
                let center = point(
                    ((left + right - dot) / 2.).round(),
                    (row.y + (metrics.line_height - dot) / 2.).round(),
                );
                symbols.spaces.push(Bounds::new(center, size(dot, dot)));
            } else {
                symbols.tabs.push(Bounds::from_corners(
                    point(left, row.y),
                    point(right, row.y + metrics.line_height),
                ));
            }
        }
    }
}

/// x of display offsets of a shaped row, asked for in increasing order: one pass over the
/// glyphs where `ShapedLine::x_for_index` scans from the start each time.
struct GlyphCursor<'a> {
    row: &'a VisibleRow,
    glyphs: Vec<(usize, Pixels)>,
    next: usize,
}

impl<'a> GlyphCursor<'a> {
    fn new(row: &'a VisibleRow) -> Self {
        let glyphs = row
            .shaped
            .runs
            .iter()
            .flat_map(|run| run.glyphs.iter())
            .map(|glyph| (glyph.index, glyph.position.x))
            .collect();
        Self {
            row,
            glyphs,
            next: 0,
        }
    }

    fn x_at(&mut self, offset: usize) -> Pixels {
        while self
            .glyphs
            .get(self.next)
            .is_some_and(|&(index, _)| index < offset)
        {
            self.next += 1;
        }
        let x = self
            .glyphs
            .get(self.next)
            .map_or(self.row.shaped.width(), |&(_, x)| x);
        self.row.x + x
    }
}

#[cfg(test)]
#[path = "../tests/unit/symbols/tests.rs"]
mod tests;
