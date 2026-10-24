//! Turning the cell layout of `birchpad-view` into pixels for one frame: which rows are visible,
//! their shaped text, selections, carets, line numbers and scrollbars.
//!
//! Only visible rows are shaped, and of a long row only the columns in view: a 10 MB line costs
//! the same per frame as a short one (once its column index exists).

use std::ops::Range as ByteRange;

use birchpad_core::motion::{line_count, line_range};
use birchpad_core::{LineChange, Rope};
use birchpad_view::{BlockPoint, DisplayMap, DisplayText, LayoutConfig, Row};
use gpui_kit::{
    App, AppContext as _, Bounds, Context, Font, FontStyle, FontWeight, Hsla, Pixels, Point,
    ShapedLine, TextRun, UnderlineStyle, Window, font, point, px, rgba, size,
};

use super::symbols::Symbols;
use super::theme::{self, Paint, TextStyle};
use super::{EditorView, ViewSettings};

/// Documents larger than this are rewrapped on a background thread.
const BACKGROUND_WRAP: usize = 4 * 1024 * 1024;
/// Rows longer than this many bytes are shaped only around the visible columns.
const LONG_ROW: usize = 2048;
/// Extra columns shaped on each side of the visible ones in a long row.
const LONG_ROW_MARGIN: usize = 16;
pub const SCROLLBAR: Pixels = px(12.);
const TEXT_PADDING: Pixels = px(4.);
/// Columns kept between the caret and the left or right edge when scrolling horizontally.
const CARET_MARGIN_COLUMNS: f32 = 4.;

#[derive(Debug, Clone)]
pub struct Metrics {
    pub font: Font,
    pub font_size: Pixels,
    pub line_height: Pixels,
    /// Width of one cell of the monospace grid.
    pub cell: Pixels,
}

/// Gaps smaller than this between the parts of the text shown in a frame are highlighted with
/// one query; larger ones (inside a long, partly shown line) split the query.
const QUERY_GAP: usize = 4096;

/// Syntax highlights of the text shown in the last frame, reused while the tree and the
/// visible text stay the same (as when the caret blinks).
pub struct HighlightCache {
    language: &'static str,
    version: u64,
    /// The theme's generation: a new theme colors the text again.
    theme: u64,
    ranges: Vec<ByteRange<usize>>,
    spans: Vec<(ByteRange<usize>, TextStyle)>,
}

/// How the text of a frame is colored: syntax highlights and the brackets at the caret.
#[derive(Default)]
struct FrameStyles {
    /// Sorted, non-overlapping.
    spans: Vec<(ByteRange<usize>, TextStyle)>,
    braces: Vec<(ByteRange<usize>, TextStyle)>,
}

impl FrameStyles {
    /// The style at document position `pos`: a bracket's, else the highlight's.
    fn at(&self, pos: usize) -> Option<TextStyle> {
        if let Some((_, style)) = self.braces.iter().find(|(range, _)| range.contains(&pos)) {
            return Some(*style);
        }
        let index = self.spans.partition_point(|(range, _)| range.end <= pos);
        self.spans
            .get(index)
            .filter(|(range, _)| range.start <= pos)
            .map(|(_, style)| *style)
    }

    /// Positions where the style may change, within `range`.
    fn boundaries(&self, range: ByteRange<usize>) -> impl Iterator<Item = usize> + '_ {
        let first = self.spans.partition_point(|(r, _)| r.end <= range.start);
        self.spans[first..]
            .iter()
            .take_while(move |(r, _)| r.start < range.end)
            .chain(self.braces.iter())
            .flat_map(|(r, _)| [r.start, r.end])
    }
}

/// A row painted in this frame.
pub struct VisibleRow {
    pub row: Row,
    /// Top of the row, in window coordinates.
    pub y: Pixels,
    /// Where `display` starts, in window coordinates.
    pub x: Pixels,
    pub display: DisplayText,
    pub shaped: ShapedLine,
    /// The order the characters show in, when it is not the order they are stored in.
    pub bidi: Option<birchpad_view::BidiRow>,
}

impl VisibleRow {
    /// Whether the shaped text covers the whole row (short rows) or only the visible part.
    fn covers(&self, pos: usize) -> bool {
        self.display.doc_start() <= pos && pos <= self.display.doc_end()
    }

    pub fn x_for(&self, pos: usize) -> Option<Pixels> {
        self.covers(pos).then(|| {
            let index = self.display.to_display(pos);
            let index = self
                .bidi
                .as_ref()
                .map_or(index, |bidi| bidi.visual_caret(index));
            self.x + self.shaped.x_for_index(index)
        })
    }

    fn contains_caret(&self, pos: usize) -> bool {
        self.row.range.start <= pos
            && (pos < self.row.range.end || (pos == self.row.range.end && self.row.last_in_line))
    }
}

pub struct Scrollbar {
    pub track: Bounds<Pixels>,
    pub thumb: Bounds<Pixels>,
    /// Marks along the track: where Compare found differences (its navigation bar).
    pub markers: Vec<(Bounds<Pixels>, birchpad_theme::Color)>,
}

/// The margins left of the text, as in Notepad++: line numbers, symbols (bookmarks), the change
/// history, folding. A margin that is turned off is `None`.
pub struct Margins {
    /// All margins together.
    pub gutter: Bounds<Pixels>,
    pub line_numbers: Option<Bounds<Pixels>>,
    pub symbols: Option<Bounds<Pixels>>,
    pub changes: Option<Bounds<Pixels>>,
    pub folding: Option<Bounds<Pixels>>,
}

impl Margins {
    /// The margins at the left of `bounds`, or in right-to-left mode (`rtl`) mirrored at the
    /// right of the text, which ends at `right`: line numbers outermost, folding next to the
    /// text, as Scintilla mirrors them.
    fn new(
        bounds: Bounds<Pixels>,
        settings: &ViewSettings,
        metrics: &Metrics,
        line_count: usize,
        rtl: Option<Pixels>,
    ) -> Self {
        let mut x = rtl.unwrap_or(bounds.left());
        let mut column = |enabled: bool, width: Pixels| {
            enabled.then(|| {
                let left = if rtl.is_some() { x - width } else { x };
                let margin =
                    Bounds::new(point(left, bounds.top()), size(width, bounds.size.height));
                if rtl.is_some() {
                    x -= width;
                } else {
                    x += width;
                }
                margin
            })
        };
        // Wide enough for the largest line number (at least three digits).
        let digits = line_count.to_string().len().max(3);
        let line_numbers = column(settings.line_numbers, metrics.cell * (digits as f32 + 2.));
        let symbols = column(settings.bookmark_margin, metrics.line_height);
        let changes = column(
            settings.change_history,
            (metrics.line_height * 0.3).round().max(px(4.)),
        );
        let folding = column(settings.fold_margin, (metrics.line_height * 0.8).round());
        let gutter = match rtl {
            Some(right) => {
                Bounds::from_corners(point(x, bounds.top()), point(right, bounds.bottom()))
            }
            None => Bounds::from_corners(bounds.origin, point(x, bounds.bottom())),
        };
        Self {
            gutter,
            line_numbers,
            symbols,
            changes,
            folding,
        }
    }
}

/// A line sticky scroll keeps at the top of the view.
pub struct StickyRow {
    pub line: usize,
    /// Top of the row, in window coordinates.
    pub y: Pixels,
    /// Where its text starts.
    pub x: Pixels,
    pub shaped: ShapedLine,
    /// Its number in the line number margin.
    pub number: Option<(Point<Pixels>, ShapedLine)>,
}

pub struct Layout {
    pub metrics: Metrics,
    pub margins: Margins,
    pub text_bounds: Bounds<Pixels>,
    /// Window x of column 0 of an unwrapped row (scrolled).
    pub column_zero: Pixels,
    pub rows: Vec<VisibleRow>,
    pub line_numbers: Vec<(Point<Pixels>, ShapedLine)>,
    /// Bookmark symbols in the symbol margin.
    pub bookmarks: Vec<Bounds<Pixels>>,
    /// Bars of changed lines in the change history margin.
    pub change_bars: Vec<(Bounds<Pixels>, LineChange)>,
    /// Fold boxes in the folding margin, and whether each fold is collapsed.
    pub fold_boxes: Vec<(Bounds<Pixels>, bool)>,
    /// Lines with hidden lines below them (View > Hide Lines), with the band across the
    /// margins and the text that marks them; a click in the margins shows them again.
    pub hidden_marks: Vec<(usize, Bounds<Pixels>)>,
    /// The lines of expanded folds in the folding margin.
    pub fold_lines: Vec<Bounds<Pixels>>,
    /// A line under each collapsed fold's header, across the text.
    pub fold_underlines: Vec<Bounds<Pixels>>,
    /// Decoration ranges (token styles, matches) drawn behind or around the text.
    pub decorations: Vec<(Bounds<Pixels>, Paint)>,
    pub selections: Vec<Bounds<Pixels>>,
    pub carets: Vec<Bounds<Pixels>>,
    pub vertical_bar: Option<Scrollbar>,
    pub horizontal_bar: Option<Scrollbar>,
    /// Show Symbol, the edge and the current line.
    pub symbols: Symbols,
    pub total_rows: usize,
    /// Rows that fit completely.
    pub full_rows: usize,
    /// Sticky scroll's lines, from the top.
    pub sticky: Vec<StickyRow>,
}

impl EditorView {
    /// Computes the frame's layout for `bounds`, applying pending scroll requests.
    pub fn compute_layout(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.track_focus(window, cx);
        let settings = self.view_settings(cx);
        let text = self.text(cx).clone();
        let metrics = metrics(&settings, window);
        let line_height = metrics.line_height;
        let cell = metrics.cell;

        // Right to left: the margins at the right, between the text and the scrollbar.
        let right_edge = bounds.right() - SCROLLBAR;
        let margins = Margins::new(
            bounds,
            &settings,
            &metrics,
            line_count(&text),
            self.rtl.then_some(right_edge),
        );
        let mut text_bounds = if self.rtl {
            Bounds::from_corners(bounds.origin, point(margins.gutter.left(), bounds.bottom()))
        } else {
            Bounds::from_corners(
                point(margins.gutter.right(), bounds.top()),
                point(right_edge, bounds.bottom()),
            )
        };

        let wrap_width = settings.word_wrap.then(|| {
            let usable = text_bounds.size.width - TEXT_PADDING * 2.;
            // The wrap symbol takes the last column.
            let symbol = usize::from(settings.wrap_symbol);
            ((usable / cell).floor() as usize)
                .saturating_sub(symbol)
                .max(8)
        });
        self.update_display_config(
            &text,
            LayoutConfig {
                tab_width: settings.tab_width,
                wrap_width,
            },
            cx,
        );
        self.sync_hidden(cx);
        let wrap_width = self.display.config().wrap_width;
        let show_horizontal = wrap_width.is_none() && self.scroll_width > text_bounds.size.width;
        if show_horizontal {
            text_bounds.size.height -= SCROLLBAR;
        }
        if wrap_width.is_some() {
            self.scroll_left = px(0.);
        }

        let total_rows = self.display.row_count(&text);
        let full_rows = ((text_bounds.size.height / line_height).floor() as usize).max(1);
        let primary_range = self.selection.primary();
        let primary = primary_range.head;

        if self.autoscroll {
            let (caret_row, row) = self.display.row_of(&text, primary);
            let caret_row = caret_row as f64;
            if caret_row < self.scroll_top {
                self.scroll_top = caret_row;
            } else if caret_row + 1. > self.scroll_top + full_rows as f64 {
                self.scroll_top = caret_row + 1. - full_rows as f64;
            }
            if wrap_width.is_none() {
                // Columns are a good enough estimate of x for keeping the caret in view.
                let column = self.display.column(&text, primary) + primary_range.head_virtual
                    - row.start_column;
                let caret_x = cell * column as f32;
                let margin = cell * CARET_MARGIN_COLUMNS;
                let visible = text_bounds.size.width - TEXT_PADDING * 2.;
                if caret_x < self.scroll_left + margin {
                    self.scroll_left = (caret_x - margin).max(px(0.));
                } else if caret_x > self.scroll_left + visible - margin {
                    self.scroll_left = caret_x - visible + margin;
                }
                self.scroll_width = self.scroll_width.max(caret_x + margin * 2.);
            }
            self.autoscroll = false;
        }
        let max_top = total_rows.saturating_sub(full_rows) as f64;
        self.scroll_top = self.scroll_top.clamp(0., max_top);

        let column_zero = text_bounds.left() + TEXT_PADDING - self.scroll_left;
        let first_row = self.scroll_top.floor() as usize;
        let offset = line_height * (self.scroll_top - first_row as f64) as f32;
        let visible_count = full_rows + 2;

        // First the rows and the part of each that is shown, then their highlights, then shaping.
        let mut pending = Vec::with_capacity(visible_count);
        let mut line_numbers = Vec::new();
        let mut widest = px(0.);
        for index in first_row..(first_row + visible_count).min(total_rows) {
            let row = self.display.row(&text, index);
            let y = text_bounds.top() + line_height * (index - first_row) as f32 - offset;
            let (display, x) = if row.range.len() > LONG_ROW {
                // Only the columns in view, positioned by cells.
                let first_column = row.start_column
                    + ((self.scroll_left / cell).floor() as usize).saturating_sub(LONG_ROW_MARGIN);
                let last_column = first_column
                    + (text_bounds.size.width / cell).ceil() as usize
                    + 2 * LONG_ROW_MARGIN;
                let start = self.display.pos_at_column(&text, &row, first_column);
                let end = self.display.pos_at_column(&text, &row, last_column);
                let start_column = self.display.column(&text, start);
                let row_cells = self.display.column(&text, row.range.end) - row.start_column;
                widest = widest.max(cell * row_cells as f32);
                let display = DisplayText::with_pictures(
                    &text,
                    start..end,
                    start_column,
                    settings.tab_width,
                    settings.show_control_chars,
                );
                let x = column_zero + cell * (start_column - row.start_column) as f32;
                (display, x)
            } else {
                let display = DisplayText::with_pictures(
                    &text,
                    row.range.clone(),
                    row.start_column,
                    settings.tab_width,
                    settings.show_control_chars,
                );
                let x = if wrap_width.is_some() {
                    text_bounds.left() + TEXT_PADDING
                } else {
                    column_zero
                };
                (display, x)
            };
            if row.index_in_line == 0
                && let Some(margin) = margins.line_numbers
            {
                let number = (row.line + 1).to_string();
                let shaped_number = shape(
                    window,
                    &number,
                    &metrics,
                    theme::paint(theme::editor().gutter_text).into(),
                );
                let x = margin.right() - cell - shaped_number.width();
                line_numbers.push((point(x, y), shaped_number));
            }
            pending.push((row, y, x, display));
        }
        let shown = shown_ranges(&pending);
        if self.update_smart_highlight(&shown, &text, cx) {
            // Searching continues in the next frame.
            window.request_animation_frame();
        }
        // Sticky scroll: the headers of the folds around the top, highlighted with the rest.
        let sticky_lines = match settings.sticky_scroll {
            Some(max) if !self.rtl && self.scroll_top > 0. => {
                let mut lines: Vec<usize> = pending.iter().map(|(row, ..)| row.line).collect();
                lines.dedup();
                let folds = self.folds(cx).to_vec();
                birchpad_view::sticky_headers(&folds, &lines, max.min(full_rows / 2))
            }
            _ => Vec::new(),
        };
        let sticky_ranges: Vec<ByteRange<usize>> = sticky_lines
            .iter()
            .map(|&line| {
                let range = line_range(&text, line);
                let end = text.floor_char_boundary(range.end.min(range.start + LONG_ROW));
                range.start..end
            })
            .collect();
        let styles = if sticky_ranges.is_empty() {
            self.frame_styles(&shown, &text, cx)
        } else {
            self.frame_styles(&[sticky_ranges.clone(), shown.clone()].concat(), &text, cx)
        };
        let mut rows = Vec::with_capacity(pending.len());
        for (row, y, mut x, display) in pending {
            // Rows shaped only in part (very long ones) reorder the part shown.
            let bidi = birchpad_view::BidiRow::new(&display.text, self.rtl);
            let shaped = match &bidi {
                Some(bidi) => self.shape_bidi_row(&display, bidi, &metrics, &styles, window),
                None => self.shape_row(&display, &metrics, &styles, window),
            };
            if row.range.len() <= LONG_ROW {
                widest = widest.max(shaped.width());
            }
            // Right to left: rows that fit end at the right edge.
            let room = text_bounds.size.width - TEXT_PADDING * 2.;
            if self.rtl && row.range.len() <= LONG_ROW && shaped.width() <= room {
                x = text_bounds.right() - TEXT_PADDING - shaped.width();
            }
            rows.push(VisibleRow {
                row,
                y,
                x,
                display,
                shaped,
                bidi,
            });
        }
        let mut sticky = Vec::with_capacity(sticky_lines.len());
        for (index, (line, range)) in sticky_lines.into_iter().zip(sticky_ranges).enumerate() {
            let display = DisplayText::with_pictures(
                &text,
                range,
                0,
                settings.tab_width,
                settings.show_control_chars,
            );
            let shaped = self.shape_row(&display, &metrics, &styles, window);
            let y = text_bounds.top() + line_height * index as f32;
            let x = if wrap_width.is_some() {
                text_bounds.left() + TEXT_PADDING
            } else {
                column_zero
            };
            let number = margins.line_numbers.map(|margin| {
                let shaped = shape(
                    window,
                    &(line + 1).to_string(),
                    &metrics,
                    theme::paint(theme::editor().gutter_text).into(),
                );
                (point(margin.right() - cell - shaped.width(), y), shaped)
            });
            sticky.push(StickyRow {
                line,
                y,
                x,
                shaped,
                number,
            });
        }
        if wrap_width.is_none() {
            self.scroll_width = self.scroll_width.max(widest + cell * 2.);
            let max_left = (self.scroll_width - text_bounds.size.width).max(px(0.));
            self.scroll_left = self.scroll_left.min(max_left);
        }

        let mut layout = Layout {
            metrics,
            margins,
            text_bounds,
            column_zero,
            rows,
            line_numbers,
            bookmarks: Vec::new(),
            change_bars: Vec::new(),
            fold_boxes: Vec::new(),
            hidden_marks: Vec::new(),
            fold_lines: Vec::new(),
            fold_underlines: Vec::new(),
            decorations: Vec::new(),
            selections: Vec::new(),
            carets: Vec::new(),
            vertical_bar: None,
            horizontal_bar: None,
            symbols: Symbols::default(),
            total_rows,
            full_rows,
            sticky,
        };
        layout.bookmarks = self.bookmark_symbols(&layout, &text, cx);
        layout.change_bars = self.change_bars(&layout, &text, cx);
        self.fold_marks(&mut layout, &text);
        self.hidden_marks(&mut layout, &text);
        layout.decorations = self.decoration_bounds(&layout, &text, cx);
        layout.selections = self.selection_bounds(&layout, &text);
        layout.carets = self.caret_bounds(&layout, &text);
        self.layout_symbols(&mut layout, &text, &settings, window);
        layout.vertical_bar = vertical_scrollbar(bounds, &layout, self.scroll_top);
        if let Some(bar) = &mut layout.vertical_bar {
            bar.markers = self.difference_markers(bar.track, layout.total_rows, &text, cx);
        }
        if show_horizontal {
            layout.horizontal_bar = Some(horizontal_scrollbar(
                &layout,
                self.scroll_left,
                self.scroll_width,
            ));
        }
        self.layout = Some(layout);
        self.report_scroll(cx);
    }

    /// Applies a new tab width or wrap width. Large documents are rewrapped in the background;
    /// until that finishes the previous layout stays on screen.
    fn update_display_config(&mut self, text: &Rope, config: LayoutConfig, cx: &mut Context<Self>) {
        if config == self.display.config() {
            self.wrap_job = None;
            return;
        }
        if config.wrap_width.is_none() || text.len() < BACKGROUND_WRAP {
            self.display.set_config(text, config);
            self.wrap_job = None;
            return;
        }
        let revision = self.buffer.read(cx).doc().revision();
        if self
            .wrap_job
            .as_ref()
            .is_some_and(|job| job.config == config && job.revision == revision)
        {
            return;
        }
        let snapshot = text.clone();
        let task = cx.spawn(async move |this, cx| {
            let rows = cx
                .background_spawn(async move { DisplayMap::compute_rows(&snapshot, config) })
                .await;
            this.update(cx, |view, cx| {
                let current = view.buffer.read(cx).doc().revision();
                if view
                    .wrap_job
                    .as_ref()
                    .is_some_and(|job| job.config == config && job.revision == current)
                {
                    view.display.install(config, rows);
                    view.autoscroll = true;
                }
                // Otherwise the text or the width changed meanwhile; the next frame starts over.
                view.wrap_job = None;
                cx.notify();
            })
            .ok();
        });
        self.wrap_job = Some(super::WrapJob {
            config,
            revision,
            _task: task,
        });
    }

    /// Highlights and bracket styles for the text shown in this frame (`ranges`, from
    /// [`shown_ranges`]).
    fn frame_styles(&mut self, ranges: &[ByteRange<usize>], text: &Rope, cx: &App) -> FrameStyles {
        let buffer = self.buffer.read(cx);
        let mut styles = FrameStyles::default();
        if let Some(found) =
            birchpad_syntax::matching_bracket(text, buffer.syntax(), self.selection.primary().head)
        {
            match found.partner {
                Some(partner) => {
                    styles.braces.push((found.bracket, theme::brace_match()));
                    styles.braces.push((partner, theme::brace_match()));
                }
                None => styles.braces.push((found.bracket, theme::brace_bad())),
            }
        }
        let Some(syntax) = buffer.syntax() else {
            self.highlight_cache = None;
            if let Some(language) = buffer.user_language() {
                let end = ranges.iter().map(|range| range.end).max().unwrap_or(0);
                let pieces = buffer.udl_pieces(end);
                for (piece, kind) in pieces.iter() {
                    let visible = ranges
                        .iter()
                        .any(|range| piece.start < range.end && piece.end > range.start);
                    if let (true, Some(style)) = (
                        visible,
                        (crate::EditorHooks::get(cx).user_language_style)(language, *kind),
                    ) {
                        styles.spans.push((piece.clone(), style));
                    }
                }
            }
            return styles;
        };
        let language = syntax.language().id;
        let version = syntax.version();
        let generation = theme::generation();
        let cached = self.highlight_cache.as_ref().is_some_and(|cache| {
            cache.language == language
                && cache.version == version
                && cache.theme == generation
                && cache.ranges == ranges
        });
        if !cached {
            let mut spans = Vec::new();
            for range in ranges {
                for (span, highlight) in syntax.highlights(text, range.clone()) {
                    if let Some(style) = theme::syntax_style(Some(language), highlight) {
                        spans.push((span, style));
                    }
                }
            }
            self.highlight_cache = Some(HighlightCache {
                language,
                version,
                theme: generation,
                ranges: ranges.to_vec(),
                spans,
            });
        }
        styles.spans = self
            .highlight_cache
            .as_ref()
            .map(|cache| cache.spans.clone())
            .unwrap_or_default();
        styles
    }

    /// Shapes a row in the order its characters show, each colored as where it is stored.
    fn shape_bidi_row(
        &self,
        display: &DisplayText,
        bidi: &birchpad_view::BidiRow,
        metrics: &Metrics,
        styles: &FrameStyles,
        window: &mut Window,
    ) -> ShapedLine {
        let color: Hsla = theme::paint(theme::editor().text).into();
        let mut runs: Vec<TextRun> = Vec::new();
        let mut last: Option<Option<TextStyle>> = None;
        for range in bidi.visual_chars() {
            let style = styles.at(display.to_doc(range.start));
            if last == Some(style)
                && let Some(run) = runs.last_mut()
            {
                run.len += range.len();
                continue;
            }
            let mut run = TextRun {
                len: range.len(),
                font: metrics.font.clone(),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            if let Some(style) = style {
                if let Some(color) = style.color {
                    run.color = rgba(color).into();
                }
                run.background_color = style.background.map(|color| rgba(color).into());
                if style.bold {
                    run.font.weight = FontWeight::BOLD;
                }
                if style.italic {
                    run.font.style = FontStyle::Italic;
                }
                if style.underline {
                    run.underline = Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.),
                        wavy: false,
                    });
                }
            }
            runs.push(run);
            last = Some(style);
        }
        window
            .text_system()
            .shape_line(bidi.visual.clone().into(), metrics.font_size, &runs, None)
    }

    fn shape_row(
        &self,
        display: &DisplayText,
        metrics: &Metrics,
        styles: &FrameStyles,
        window: &mut Window,
    ) -> ShapedLine {
        let color: Hsla = theme::paint(theme::editor().text).into();
        let base = TextRun {
            len: display.text.len(),
            font: metrics.font.clone(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let (doc_start, doc_end) = (display.doc_start(), display.doc_end());
        let marked = self
            .marked
            .clone()
            .filter(|marked| marked.start < doc_end && marked.end > doc_start);
        // Cut the row wherever the style changes, in display offsets.
        let to_display = |pos: usize| display.to_display(pos.clamp(doc_start, doc_end));
        let mut cuts: Vec<usize> = styles
            .boundaries(doc_start..doc_end)
            .chain(marked.iter().flat_map(|m| [m.start, m.end]))
            .map(to_display)
            .chain([0, display.text.len()])
            .collect();
        cuts.sort_unstable();
        cuts.dedup();
        let mut runs: Vec<TextRun> = cuts
            .windows(2)
            .filter(|cut| cut[0] < cut[1])
            .map(|cut| {
                let pos = display.to_doc(cut[0]);
                let mut run = TextRun {
                    len: cut[1] - cut[0],
                    ..base.clone()
                };
                if let Some(style) = styles.at(pos) {
                    if let Some(color) = style.color {
                        run.color = rgba(color).into();
                    }
                    run.background_color = style.background.map(|color| rgba(color).into());
                    if style.underline {
                        run.underline = Some(UnderlineStyle {
                            color: Some(run.color),
                            thickness: px(1.),
                            wavy: false,
                        });
                    }
                    if style.bold {
                        run.font.weight = FontWeight::BOLD;
                    }
                    if style.italic {
                        run.font.style = FontStyle::Italic;
                    }
                }
                // Underline the text an input method is composing.
                if marked.as_ref().is_some_and(|m| m.contains(&pos)) {
                    run.underline = Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.),
                        wavy: false,
                    });
                }
                run
            })
            .collect();
        if runs.is_empty() {
            runs.push(base);
        }
        window
            .text_system()
            .shape_line(display.text.clone().into(), metrics.font_size, &runs, None)
    }

    /// x of `pos` in `row`, from the shaped text or, outside the shaped part of a long row, from
    /// its column.
    pub fn x_in_row(
        &mut self,
        layout: &Layout,
        row: &VisibleRow,
        pos: usize,
        text: &Rope,
    ) -> Pixels {
        row.x_for(pos).unwrap_or_else(|| {
            let column = self.display.column(text, pos) - row.row.start_column;
            layout.column_zero + layout.metrics.cell * column as f32
        })
    }

    /// x of `pos` plus `virtual_cells` past it; virtual space only counts at the end of a line.
    fn x_with_virtual(
        &mut self,
        layout: &Layout,
        row: &VisibleRow,
        pos: usize,
        virtual_cells: usize,
        text: &Rope,
    ) -> Pixels {
        let x = self.x_in_row(layout, row, pos, text);
        if virtual_cells > 0 && row.row.last_in_line && pos == row.row.range.end {
            x + layout.metrics.cell * virtual_cells as f32
        } else {
            x
        }
    }

    /// The document bytes shown in this frame.
    fn visible_range(layout: &Layout) -> ByteRange<usize> {
        match (layout.rows.first(), layout.rows.last()) {
            (Some(first), Some(last)) => first.row.range.start..last.row.range.end,
            _ => 0..0,
        }
    }

    /// Bounds of `range` in each visible row it touches (line breaks excluded).
    pub fn range_bounds(
        &mut self,
        layout: &Layout,
        text: &Rope,
        range: ByteRange<usize>,
    ) -> Vec<Bounds<Pixels>> {
        let mut quads = Vec::new();
        for row in &layout.rows {
            let from = range.start.max(row.row.range.start);
            let to = range.end.min(row.row.range.end);
            if from >= to {
                continue;
            }
            let (a, b) = (
                self.x_in_row(layout, row, from, text),
                self.x_in_row(layout, row, to, text),
            );
            let (left, right) = (a.min(b), a.max(b));
            quads.push(Bounds::from_corners(
                point(left, row.y),
                point(right, row.y + layout.metrics.line_height),
            ));
        }
        quads
    }

    fn bookmark_symbols(&self, layout: &Layout, text: &Rope, cx: &App) -> Vec<Bounds<Pixels>> {
        let Some(margin) = layout.margins.symbols else {
            return Vec::new();
        };
        let (Some(first), Some(last)) = (layout.rows.first(), layout.rows.last()) else {
            return Vec::new();
        };
        let marks = self.buffer.read(cx).marks();
        let marked = marks
            .bookmarks
            .lines_in(text, first.row.line..last.row.line + 1);
        let line_height = layout.metrics.line_height;
        let side = (margin.size.width.min(line_height) * 0.6).round();
        layout
            .rows
            .iter()
            .filter(|row| row.row.index_in_line == 0 && marked.contains(&row.row.line))
            .map(|row| {
                let origin = point(
                    margin.left() + (margin.size.width - side) / 2.,
                    row.y + (line_height - side) / 2.,
                );
                Bounds::new(origin, size(side, side))
            })
            .collect()
    }

    /// A bar beside each visible row of a changed line, in the change history margin.
    fn change_bars(
        &self,
        layout: &Layout,
        text: &Rope,
        cx: &App,
    ) -> Vec<(Bounds<Pixels>, LineChange)> {
        let Some(margin) = layout.margins.changes else {
            return Vec::new();
        };
        let (Some(first), Some(last)) = (layout.rows.first(), layout.rows.last()) else {
            return Vec::new();
        };
        let lines = first.row.line..last.row.line + 1;
        let (modified, saved) = self.buffer.read(cx).changed_lines();
        let modified = modified.lines_in(text, lines.clone());
        let saved = saved.lines_in(text, lines);
        if modified.is_empty() && saved.is_empty() {
            return Vec::new();
        }
        // The bar leaves a pixel of the margin free on each side.
        let width = (margin.size.width - px(2.)).max(px(1.));
        layout
            .rows
            .iter()
            .filter_map(|row| {
                let line = row.row.line;
                let change = if modified.binary_search(&line).is_ok() {
                    LineChange::Modified
                } else if saved.binary_search(&line).is_ok() {
                    LineChange::Saved
                } else {
                    return None;
                };
                let bar = Bounds::new(
                    point(margin.left() + px(1.), row.y),
                    size(width, layout.metrics.line_height),
                );
                Some((bar, change))
            })
            .collect()
    }

    /// A band under the last row of each line with hidden lines below it.
    fn hidden_marks(&self, layout: &mut Layout, text: &Rope) {
        let (Some(first), Some(last)) = (layout.rows.first(), layout.rows.last()) else {
            return;
        };
        let anchors = self.hidden_anchors(text, first.row.line..last.row.line + 1);
        let line_height = layout.metrics.line_height;
        // Across the margins and the text, whichever side the margins are on.
        let left = layout.margins.gutter.left().min(layout.text_bounds.left());
        let right = layout
            .text_bounds
            .right()
            .max(layout.margins.gutter.right());
        for row in layout.rows.iter().filter(|row| row.row.last_in_line) {
            if anchors.contains(&row.row.line) {
                let bottom = row.y + line_height;
                layout.hidden_marks.push((
                    row.row.line,
                    Bounds::from_corners(point(left, bottom - px(2.)), point(right, bottom)),
                ));
            }
        }
    }

    /// Fold boxes, the lines of expanded folds, and underlines of collapsed headers, for the
    /// visible rows.
    fn fold_marks(&self, layout: &mut Layout, text: &Rope) {
        let Some(margin) = layout.margins.folding else {
            return;
        };
        let (Some(first), Some(last)) = (layout.rows.first(), layout.rows.last()) else {
            return;
        };
        let (first_line, last_line) = (first.row.line, last.row.line);
        let folds = self
            .fold_cache
            .as_ref()
            .map_or(&[][..], |cache| cache.folds.as_slice());
        let collapsed = self.collapsed.lines_in(text, first_line..last_line + 1);
        // For each visible row: inside an expanded fold that goes on below it, and whether an
        // expanded fold ends on it.
        let rows = &layout.rows;
        let mut continues = vec![false; rows.len()];
        let mut ends = vec![false; rows.len()];
        let candidates = folds.partition_point(|fold| fold.header <= last_line);
        for fold in folds[..candidates]
            .iter()
            .filter(|fold| fold.last >= first_line)
        {
            if collapsed.contains(&fold.header) {
                continue;
            }
            for (index, row) in rows.iter().enumerate() {
                let line = row.row.line;
                if line <= fold.header || line > fold.last {
                    continue;
                }
                if line == fold.last && row.row.last_in_line {
                    ends[index] = true;
                } else {
                    continues[index] = true;
                }
            }
        }

        let line_height = layout.metrics.line_height;
        let center = (margin.left() + margin.size.width / 2.).round();
        let side = (margin.size.width.min(line_height) * 0.6).round();
        let thin = px(1.);
        let vertical = |top: Pixels, bottom: Pixels| {
            Bounds::from_corners(point(center, top), point(center + thin, bottom))
        };
        for (index, row) in rows.iter().enumerate() {
            let (top, bottom) = (row.y, row.y + line_height);
            let middle = (top + line_height / 2.).round();
            let header =
                row.row.index_in_line == 0 && birchpad_view::fold_at(folds, row.row.line).is_some();
            if header {
                let is_collapsed = collapsed.contains(&row.row.line);
                let fold_box = Bounds::new(
                    point(center - (side / 2.).floor(), middle - (side / 2.).floor()),
                    size(side, side),
                );
                if continues[index] || ends[index] {
                    layout.fold_lines.push(vertical(top, fold_box.top()));
                }
                if continues[index] || !is_collapsed {
                    layout.fold_lines.push(vertical(fold_box.bottom(), bottom));
                }
                layout.fold_boxes.push((fold_box, is_collapsed));
                if is_collapsed {
                    let text_bounds = layout.text_bounds;
                    layout.fold_underlines.push(Bounds::new(
                        point(text_bounds.left(), bottom - thin),
                        size(text_bounds.size.width, thin),
                    ));
                }
                continue;
            }
            if continues[index] {
                layout.fold_lines.push(vertical(top, bottom));
            } else if ends[index] {
                layout.fold_lines.push(vertical(top, middle));
            }
            if ends[index] {
                let right = margin.right() - (margin.size.width - side) / 2.;
                layout.fold_lines.push(Bounds::from_corners(
                    point(center, middle),
                    point(right, middle + thin),
                ));
            }
        }
    }

    fn decoration_bounds(
        &mut self,
        layout: &Layout,
        text: &Rope,
        cx: &App,
    ) -> Vec<(Bounds<Pixels>, Paint)> {
        let visible = Self::visible_range(layout);
        let mut ranges = Vec::new();
        if let Some(smart) = &self.smart_highlight {
            let paint = Paint::Fill(theme::paint(theme::editor().smart_highlight));
            ranges.extend(
                smart
                    .matches
                    .overlapping(visible.clone())
                    .map(|(range, _)| (range, paint)),
            );
        }
        let marks = self.buffer.read(cx).marks();
        for (index, style) in marks.styles.iter().enumerate() {
            let paint = theme::mark_style(index);
            ranges.extend(
                style
                    .overlapping(visible.clone())
                    .map(|(range, _)| (range, paint)),
            );
        }
        let found = Paint::Fill(theme::paint(theme::editor().find_mark));
        ranges.extend(
            marks
                .found
                .overlapping(visible.clone())
                .map(|(range, _)| (range, found)),
        );
        let incremental = Paint::Fill(theme::paint(theme::editor().incremental_highlight));
        ranges.extend(
            marks
                .incremental
                .overlapping(visible.clone())
                .map(|(range, _)| (range, incremental)),
        );
        // Clickable links, underlined.
        let links = &birchpad_ui::AppState::global(cx).settings.links;
        if links.enabled && links.underline {
            let paint = Paint::Underline(theme::paint(theme::ui().accent));
            let extra = links.extra_schemes();
            let lines = birchpad_core::motion::line_of(text, visible.start)
                ..birchpad_core::motion::line_of(text, visible.end) + 1;
            for line in lines {
                let bounds = birchpad_core::motion::line_range(text, line);
                let content = text.slice(bounds.clone()).to_string();
                ranges.extend(
                    birchpad_core::links::links_in(&content, &extra)
                        .into_iter()
                        .map(|link| (bounds.start + link.start..bounds.start + link.end, paint)),
                );
            }
        }
        let spell = Paint::Underline(theme::paint(theme::editor().spell_error));
        let buffer = self.buffer.read(cx);
        let misspelled = (crate::EditorHooks::get(cx).misspelled)(
            buffer,
            self.buffer.entity_id(),
            visible.clone(),
            cx,
        );
        ranges.extend(misspelled.iter().map(|range| (range.clone(), spell)));
        let marks = buffer.marks();
        let mut quads = Vec::new();
        // The blank rows that keep compared documents side by side.
        let padding = Paint::Fill(theme::paint(theme::editor().compare_padding));
        for row in layout.rows.iter().filter(|row| row.row.padding) {
            quads.push((
                Bounds::from_corners(
                    point(layout.text_bounds.left(), row.y),
                    point(
                        layout.text_bounds.right(),
                        row.y + layout.metrics.line_height,
                    ),
                ),
                padding,
            ));
        }
        // Compared lines are filled across the text area, under the other decorations.
        for (range, change) in marks.compare.overlapping(visible.clone()) {
            let color = match change {
                birchpad_tools::compare::Change::Added => theme::editor().compare_added,
                birchpad_tools::compare::Change::Removed => theme::editor().compare_removed,
                birchpad_tools::compare::Change::Changed => theme::editor().compare_changed,
            };
            let paint = Paint::Fill(theme::paint(color));
            for row in layout.rows.iter().filter(|row| !row.row.padding) {
                let start = row.row.range.start;
                if start >= range.start && start < range.end {
                    quads.push((
                        Bounds::from_corners(
                            point(layout.text_bounds.left(), row.y),
                            point(
                                layout.text_bounds.right(),
                                row.y + layout.metrics.line_height,
                            ),
                        ),
                        paint,
                    ));
                }
            }
        }
        // What differs within changed lines, over their color.
        let inline = Paint::Fill(theme::paint(theme::editor().compare_inline));
        let inline_ranges: Vec<ByteRange<usize>> = marks
            .compare_inline
            .overlapping(visible.clone())
            .map(|(range, _)| range)
            .collect();
        for range in inline_ranges {
            for bounds in self.range_bounds(layout, text, range) {
                quads.push((bounds, inline));
            }
        }
        for (range, paint) in ranges {
            for bounds in self.range_bounds(layout, text, range) {
                quads.push((bounds, paint));
            }
        }
        quads
    }

    /// The navigation bar of Compare: each difference of the document as a mark along the
    /// vertical scrollbar's track, at its rows' place among all rows; none when it is turned
    /// off (`compare.navigation-bar`) or nothing was compared.
    fn difference_markers(
        &mut self,
        track: Bounds<Pixels>,
        total_rows: usize,
        text: &Rope,
        cx: &App,
    ) -> Vec<(Bounds<Pixels>, birchpad_theme::Color)> {
        if !birchpad_ui::AppState::global(cx)
            .settings
            .compare
            .navigation_bar
        {
            return Vec::new();
        }
        let differences: Vec<(ByteRange<usize>, birchpad_tools::compare::Change)> = self
            .buffer
            .read(cx)
            .marks()
            .compare
            .overlapping(0..text.len())
            .map(|(range, change)| (range, *change))
            .collect();
        let total = total_rows.max(1) as f32;
        let inset = px(2.);
        differences
            .into_iter()
            .map(|(range, change)| {
                let first = self.display.row_of(text, range.start).0;
                let last = self.display.row_of(text, range.end.saturating_sub(1)).0;
                let top = track.top() + track.size.height * (first as f32 / total);
                let height =
                    (track.size.height * ((last + 1 - first.min(last)) as f32 / total)).max(px(2.));
                let color = match change {
                    birchpad_tools::compare::Change::Added => theme::editor().compare_added,
                    birchpad_tools::compare::Change::Removed => theme::editor().compare_removed,
                    birchpad_tools::compare::Change::Changed => theme::editor().compare_changed,
                };
                (
                    Bounds::new(
                        point(track.left() + inset, top),
                        size(track.size.width - inset * 2., height),
                    ),
                    color.with_alpha(0xff),
                )
            })
            .collect()
    }

    fn selection_bounds(&mut self, layout: &Layout, text: &Rope) -> Vec<Bounds<Pixels>> {
        let mut quads = Vec::new();
        let line_height = layout.metrics.line_height;
        let visible = Self::visible_range(layout);
        for range in self.selection.ranges().to_vec() {
            if range.is_empty() || range.to() < visible.start || range.from() > visible.end {
                continue;
            }
            for row in layout.rows.iter().filter(|row| !row.row.padding) {
                if range.to() < row.row.range.start || range.from() > row.row.range.end {
                    continue;
                }
                let selects_break = row.row.last_in_line
                    && range.from() <= row.row.range.end
                    && range.to() > row.row.range.end;
                // Over mixed directions, one piece for each run of selected characters.
                if let Some(bidi) = &row.bidi {
                    let start = range.from().max(row.row.range.start);
                    let end = range.to().min(row.row.range.end);
                    if row.covers(start) && row.covers(end) {
                        let logical = row.display.to_display(start)..row.display.to_display(end);
                        for span in bidi.visual_spans(logical) {
                            let left = row.x + row.shaped.x_for_index(span.start);
                            let right = row.x + row.shaped.x_for_index(span.end);
                            quads.push(Bounds::from_corners(
                                point(left, row.y),
                                point(right, row.y + line_height),
                            ));
                        }
                        if selects_break {
                            // The selected line break: half a cell past the row's end, on the
                            // side the paragraph runs to.
                            let half = layout.metrics.cell * 0.5;
                            let left = if self.rtl {
                                row.x - half
                            } else {
                                row.x + row.shaped.width()
                            };
                            quads.push(Bounds::from_corners(
                                point(left, row.y),
                                point(left + half, row.y + line_height),
                            ));
                        }
                        continue;
                    }
                }
                let left = if range.from() < row.row.range.start {
                    self.x_in_row(layout, row, row.row.range.start, text)
                } else {
                    self.x_with_virtual(layout, row, range.from(), range.from_virtual(), text)
                };
                let mut right = if range.to() > row.row.range.end {
                    self.x_in_row(layout, row, row.row.range.end, text)
                } else {
                    self.x_with_virtual(layout, row, range.to(), range.to_virtual(), text)
                };
                if selects_break {
                    // A selected line break shows as half a cell after the text.
                    right += layout.metrics.cell * 0.5;
                }
                // Right-to-left text puts the end of a range left of its start.
                let (left, right) = (left.min(right), left.max(right));
                if right <= left {
                    continue;
                }
                quads.push(Bounds::from_corners(
                    point(left, row.y),
                    point(right, row.y + line_height),
                ));
            }
        }
        quads
    }

    fn caret_bounds(&mut self, layout: &Layout, text: &Rope) -> Vec<Bounds<Pixels>> {
        let mut carets = Vec::new();
        let line_height = layout.metrics.line_height;
        let (Some(first), Some(last)) = (layout.rows.first(), layout.rows.last()) else {
            return carets;
        };
        let visible = first.row.range.start..=last.row.range.end;
        for range in self.selection.ranges().to_vec() {
            // Thousands of carets: only look for rows of the visible ones.
            if !visible.contains(&range.head) {
                continue;
            }
            let Some(row) = layout
                .rows
                .iter()
                .find(|row| row.contains_caret(range.head))
            else {
                continue;
            };
            let x = self.x_with_virtual(layout, row, range.head, range.head_virtual, text);
            carets.push(if self.overwrite {
                // Overwrite mode: a bar under the character that will be replaced.
                Bounds::new(
                    point(x, row.y + line_height - px(2.)),
                    size(layout.metrics.cell, px(2.)),
                )
            } else {
                Bounds::new(point(x, row.y), size(px(2.), line_height))
            });
        }
        carets
    }
}

impl Layout {
    /// Columns that fit across the text area.
    pub fn text_columns(&self) -> usize {
        let usable = self.text_bounds.size.width - TEXT_PADDING * 2.;
        ((usable / self.metrics.cell).floor() as usize).max(1)
    }

    /// The line of the sticky row at `y`, if one is there.
    pub fn sticky_at(&self, y: Pixels) -> Option<usize> {
        let height = self.metrics.line_height;
        self.sticky
            .iter()
            .find(|row| row.y <= y && y < row.y + height)
            .map(|row| row.line)
    }

    /// The visible row at window y, if any.
    pub fn row_at_y(&self, y: Pixels) -> Option<&VisibleRow> {
        self.rows
            .iter()
            .find(|row| row.y <= y && y < row.y + self.metrics.line_height)
    }

    /// The document position under a window point; above or below the rows, the nearest row.
    pub fn position_for_point(
        &self,
        point: Point<Pixels>,
        text: &Rope,
        display: &mut DisplayMap,
    ) -> Option<usize> {
        let row = self
            .rows
            .iter()
            .find(|row| point.y < row.y + self.metrics.line_height)
            .or_else(|| self.rows.last())?;
        let pos = if row.covers(row.row.range.end) && row.display.doc_start() == row.row.range.start
        {
            let index = row.shaped.closest_index_for_x(point.x - row.x);
            let index = row
                .bidi
                .as_ref()
                .map_or(index, |bidi| bidi.logical_caret(index));
            row.display.to_doc(index)
        } else {
            // A long row shaped only in part: go by columns.
            let column = ((point.x - self.column_zero) / self.metrics.cell)
                .round()
                .max(0.);
            display.pos_at_column(text, &row.row, row.row.start_column + column as usize)
        };
        // The end of a wrapped row is the start of the next one: stay on this row.
        if !row.row.last_in_line && pos >= row.row.range.end && pos > row.row.range.start {
            Some(birchpad_core::motion::prev_boundary(
                text,
                row.row.range.end,
            ))
        } else {
            Some(pos.clamp(row.row.range.start, row.row.range.end))
        }
    }

    /// The rectangle corner under a window point: the line and column of the nearest
    /// character boundary, or past the end of a line, the column under the point.
    pub fn block_point_for_point(
        &self,
        point: Point<Pixels>,
        text: &Rope,
        display: &mut DisplayMap,
    ) -> Option<BlockPoint> {
        let row = self
            .rows
            .iter()
            .find(|row| point.y < row.y + self.metrics.line_height)
            .or_else(|| self.rows.last())?;
        let pos = self.position_for_point(point, text, display)?;
        let column = display.column(text, pos);
        let mut virtual_cells = 0;
        if row.row.last_in_line && pos == row.row.range.end {
            let end_x = row.x_for(pos).unwrap_or_else(|| {
                self.column_zero + self.metrics.cell * (column - row.row.start_column) as f32
            });
            virtual_cells = ((point.x - end_x) / self.metrics.cell).round().max(0.) as usize;
        }
        Some(BlockPoint::new(row.row.line, column + virtual_cells))
    }

    /// Screen bounds of a range (for input method candidate windows).
    pub fn bounds_for_range(&self, range: ByteRange<usize>) -> Option<Bounds<Pixels>> {
        let row = self
            .rows
            .iter()
            .find(|row| row.contains_caret(range.start))?;
        let start = row.x_for(range.start)?;
        let end = row.x_for(range.end.min(row.row.range.end)).unwrap_or(start);
        Some(Bounds::from_corners(
            point(start, row.y),
            point(end.max(start), row.y + self.metrics.line_height),
        ))
    }
}

/// The parts of the text shown by `rows`, merged where they are close: what highlighting and
/// smart highlighting look at.
fn shown_ranges(rows: &[(Row, Pixels, Pixels, DisplayText)]) -> Vec<ByteRange<usize>> {
    let mut ranges: Vec<ByteRange<usize>> = Vec::new();
    for (_, _, _, display) in rows {
        let shown = display.doc_start()..display.doc_end();
        match ranges.last_mut() {
            Some(last) if shown.start <= last.end + QUERY_GAP => {
                last.end = last.end.max(shown.end);
            }
            _ => ranges.push(shown),
        }
    }
    ranges
}

fn metrics(settings: &ViewSettings, window: &mut Window) -> Metrics {
    let font = font(settings.font_family.clone());
    let font_size = settings.font_size;
    let line_height = (font_size * 1.45).round();
    let text_system = window.text_system();
    let font_id = text_system.resolve_font(&font);
    let cell = text_system
        .advance(font_id, font_size, 'm')
        .map(|advance| advance.width)
        .unwrap_or(font_size * 0.6);
    Metrics {
        font,
        font_size,
        line_height,
        cell,
    }
}

fn shape(window: &mut Window, text: &str, metrics: &Metrics, color: Hsla) -> ShapedLine {
    let run = TextRun {
        len: text.len(),
        font: metrics.font.clone(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window
        .text_system()
        .shape_line(text.to_owned().into(), metrics.font_size, &[run], None)
}

fn vertical_scrollbar(
    bounds: Bounds<Pixels>,
    layout: &Layout,
    scroll_top: f64,
) -> Option<Scrollbar> {
    let track = Bounds::new(
        point(bounds.right() - SCROLLBAR, bounds.top()),
        size(SCROLLBAR, layout.text_bounds.size.height),
    );
    let total = layout.total_rows.max(1) as f32;
    let visible = (layout.full_rows as f32).min(total);
    let height = (track.size.height * (visible / total))
        .max(px(24.))
        .min(track.size.height);
    let max_top = (layout.total_rows.saturating_sub(layout.full_rows)) as f32;
    let fraction = if max_top > 0. {
        (scroll_top as f32 / max_top).clamp(0., 1.)
    } else {
        0.
    };
    let top = track.top() + (track.size.height - height) * fraction;
    Some(Scrollbar {
        track,
        thumb: Bounds::new(
            point(track.left() + px(2.), top),
            size(SCROLLBAR - px(4.), height),
        ),
        markers: Vec::new(),
    })
}

fn horizontal_scrollbar(layout: &Layout, scroll_left: Pixels, scroll_width: Pixels) -> Scrollbar {
    let text = layout.text_bounds;
    let track = Bounds::new(
        point(text.left(), text.bottom()),
        size(text.size.width, SCROLLBAR),
    );
    let width = (track.size.width * (text.size.width / scroll_width).min(1.))
        .max(px(24.))
        .min(track.size.width);
    let max_left = (scroll_width - text.size.width).max(px(1.));
    let fraction = (scroll_left / max_left).clamp(0., 1.);
    let left = track.left() + (track.size.width - width) * fraction;
    Scrollbar {
        track,
        thumb: Bounds::new(
            point(left, track.top() + px(2.)),
            size(width, SCROLLBAR - px(4.)),
        ),
        markers: Vec::new(),
    }
}
