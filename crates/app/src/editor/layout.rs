//! Turning the cell layout of `birchpad-view` into pixels for one frame: which rows are visible,
//! their shaped text, selections, carets, line numbers and scrollbars.
//!
//! Only visible rows are shaped, and of a long row only the columns in view: a 10 MB line costs
//! the same per frame as a short one (once its column index exists).

use std::ops::Range as ByteRange;

use birchpad_core::Rope;
use birchpad_core::motion::line_count;
use birchpad_view::{DisplayMap, DisplayText, LayoutConfig, Row};
use gpui_kit::{
    AppContext as _, Bounds, Context, Font, Hsla, Pixels, Point, ShapedLine, TextRun,
    UnderlineStyle, Window, font, point, px, rgb, size,
};

use super::{EditorView, ViewSettings};

/// Documents larger than this are rewrapped on a background thread.
const BACKGROUND_WRAP: usize = 4 * 1024 * 1024;
/// Rows longer than this many bytes are shaped only around the visible columns.
const LONG_ROW: usize = 2048;
/// Extra columns shaped on each side of the visible ones in a long row.
const LONG_ROW_MARGIN: usize = 16;
pub(super) const SCROLLBAR: Pixels = px(12.);
const TEXT_PADDING: Pixels = px(4.);
/// Columns kept between the caret and the left or right edge when scrolling horizontally.
const CARET_MARGIN_COLUMNS: f32 = 4.;

#[derive(Debug, Clone)]
pub(super) struct Metrics {
    pub(super) font: Font,
    pub(super) font_size: Pixels,
    pub(super) line_height: Pixels,
    /// Width of one cell of the monospace grid.
    pub(super) cell: Pixels,
}

/// A row painted in this frame.
pub(super) struct VisibleRow {
    pub(super) row: Row,
    /// Top of the row, in window coordinates.
    pub(super) y: Pixels,
    /// Where `display` starts, in window coordinates.
    pub(super) x: Pixels,
    pub(super) display: DisplayText,
    pub(super) shaped: ShapedLine,
}

impl VisibleRow {
    /// Whether the shaped text covers the whole row (short rows) or only the visible part.
    fn covers(&self, pos: usize) -> bool {
        self.display.doc_start() <= pos && pos <= self.display.doc_end()
    }

    fn x_for(&self, pos: usize) -> Option<Pixels> {
        self.covers(pos)
            .then(|| self.x + self.shaped.x_for_index(self.display.to_display(pos)))
    }

    fn contains_caret(&self, pos: usize) -> bool {
        self.row.range.start <= pos
            && (pos < self.row.range.end || (pos == self.row.range.end && self.row.last_in_line))
    }
}

pub(super) struct Scrollbar {
    pub(super) track: Bounds<Pixels>,
    pub(super) thumb: Bounds<Pixels>,
}

pub(super) struct Layout {
    pub(super) metrics: Metrics,
    pub(super) gutter: Bounds<Pixels>,
    pub(super) text_bounds: Bounds<Pixels>,
    /// Window x of column 0 of an unwrapped row (scrolled).
    pub(super) column_zero: Pixels,
    pub(super) rows: Vec<VisibleRow>,
    pub(super) line_numbers: Vec<(Point<Pixels>, ShapedLine)>,
    pub(super) selections: Vec<Bounds<Pixels>>,
    pub(super) carets: Vec<Bounds<Pixels>>,
    pub(super) vertical_bar: Option<Scrollbar>,
    pub(super) horizontal_bar: Option<Scrollbar>,
    pub(super) total_rows: usize,
    /// Rows that fit completely.
    pub(super) full_rows: usize,
}

pub(super) const TEXT_COLOR: u32 = 0x1f2328;
const GUTTER_TEXT: u32 = 0x8c959f;

impl EditorView {
    /// Computes the frame's layout for `bounds`, applying pending scroll requests.
    pub(super) fn compute_layout(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.track_focus(window, cx);
        let settings = ViewSettings::read(cx);
        let text = self.text(cx).clone();
        let metrics = metrics(&settings, window);
        let line_height = metrics.line_height;
        let cell = metrics.cell;

        // Gutter wide enough for the largest line number (at least three digits).
        let digits = line_count(&text).to_string().len().max(3);
        let gutter_width = cell * (digits as f32 + 2.);
        let gutter = Bounds::new(bounds.origin, size(gutter_width, bounds.size.height));
        let text_left = bounds.left() + gutter_width;
        let mut text_bounds = Bounds::from_corners(
            point(text_left, bounds.top()),
            point(bounds.right() - SCROLLBAR, bounds.bottom()),
        );

        let wrap_width = settings.word_wrap.then(|| {
            let usable = text_bounds.size.width - TEXT_PADDING * 2.;
            ((usable / cell).floor() as usize).max(8)
        });
        self.update_display_config(
            &text,
            LayoutConfig {
                tab_width: settings.tab_width,
                wrap_width,
            },
            cx,
        );
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
        let primary = self.selection.primary().head;

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
                let column = self.display.column(&text, primary) - row.start_column;
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

        let mut rows = Vec::with_capacity(visible_count);
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
                let display = DisplayText::new(&text, start..end, start_column, settings.tab_width);
                let x = column_zero + cell * (start_column - row.start_column) as f32;
                (display, x)
            } else {
                let display = DisplayText::new(
                    &text,
                    row.range.clone(),
                    row.start_column,
                    settings.tab_width,
                );
                let x = if wrap_width.is_some() {
                    text_bounds.left() + TEXT_PADDING
                } else {
                    column_zero
                };
                (display, x)
            };
            let shaped = self.shape_row(&display, &metrics, window);
            if row.range.len() <= LONG_ROW {
                widest = widest.max(shaped.width());
            }
            if row.index_in_line == 0 {
                let number = (row.line + 1).to_string();
                let shaped_number = shape(window, &number, &metrics, rgb(GUTTER_TEXT).into());
                let x = gutter.right() - cell - shaped_number.width();
                line_numbers.push((point(x, y), shaped_number));
            }
            rows.push(VisibleRow {
                row,
                y,
                x,
                display,
                shaped,
            });
        }
        if wrap_width.is_none() {
            self.scroll_width = self.scroll_width.max(widest + cell * 2.);
            let max_left = (self.scroll_width - text_bounds.size.width).max(px(0.));
            self.scroll_left = self.scroll_left.min(max_left);
        }

        let mut layout = Layout {
            metrics,
            gutter,
            text_bounds,
            column_zero,
            rows,
            line_numbers,
            selections: Vec::new(),
            carets: Vec::new(),
            vertical_bar: None,
            horizontal_bar: None,
            total_rows,
            full_rows,
        };
        layout.selections = self.selection_bounds(&layout, &text);
        layout.carets = self.caret_bounds(&layout, &text);
        layout.vertical_bar = vertical_scrollbar(bounds, &layout, self.scroll_top);
        if show_horizontal {
            layout.horizontal_bar = Some(horizontal_scrollbar(
                &layout,
                self.scroll_left,
                self.scroll_width,
            ));
        }
        self.layout = Some(layout);
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

    fn shape_row(
        &self,
        display: &DisplayText,
        metrics: &Metrics,
        window: &mut Window,
    ) -> ShapedLine {
        let color: Hsla = rgb(TEXT_COLOR).into();
        let base = TextRun {
            len: display.text.len(),
            font: metrics.font.clone(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        // Underline the text an input method is composing.
        let mut runs = Vec::new();
        if let Some(marked) = &self.marked {
            let start =
                display.to_display(marked.start.clamp(display.doc_start(), display.doc_end()));
            let end = display.to_display(marked.end.clamp(display.doc_start(), display.doc_end()));
            if start < end {
                runs.push(TextRun {
                    len: start,
                    ..base.clone()
                });
                runs.push(TextRun {
                    len: end - start,
                    underline: Some(UnderlineStyle {
                        color: Some(color),
                        thickness: px(1.),
                        wavy: false,
                    }),
                    ..base.clone()
                });
                runs.push(TextRun {
                    len: display.text.len() - end,
                    ..base.clone()
                });
                runs.retain(|run| run.len > 0);
            }
        }
        if runs.is_empty() {
            runs.push(base);
        }
        window
            .text_system()
            .shape_line(display.text.clone().into(), metrics.font_size, &runs, None)
    }

    /// x of `pos` in `row`, from the shaped text or, outside the shaped part of a long row, from
    /// its column.
    fn x_in_row(&mut self, layout: &Layout, row: &VisibleRow, pos: usize, text: &Rope) -> Pixels {
        row.x_for(pos).unwrap_or_else(|| {
            let column = self.display.column(text, pos) - row.row.start_column;
            layout.column_zero + layout.metrics.cell * column as f32
        })
    }

    fn selection_bounds(&mut self, layout: &Layout, text: &Rope) -> Vec<Bounds<Pixels>> {
        let mut quads = Vec::new();
        let line_height = layout.metrics.line_height;
        for range in self.selection.ranges().to_vec() {
            if range.is_empty() {
                continue;
            }
            for row in &layout.rows {
                let from = range.from().max(row.row.range.start);
                let to = range.to().min(row.row.range.end);
                let selects_break = row.row.last_in_line
                    && range.from() <= row.row.range.end
                    && range.to() > row.row.range.end;
                if from > to || (from == to && !selects_break) {
                    continue;
                }
                let left = self.x_in_row(layout, row, from, text);
                let mut right = self.x_in_row(layout, row, to, text);
                if selects_break {
                    // A selected line break shows as half a cell after the text.
                    right += layout.metrics.cell * 0.5;
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
        for range in self.selection.ranges().to_vec() {
            let Some(row) = layout
                .rows
                .iter()
                .find(|row| row.contains_caret(range.head))
            else {
                continue;
            };
            let x = self.x_in_row(layout, row, range.head, text);
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
    /// The document position under a window point; above or below the rows, the nearest row.
    pub(super) fn position_for_point(
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

    /// Screen bounds of a range (for input method candidate windows).
    pub(super) fn bounds_for_range(&self, range: ByteRange<usize>) -> Option<Bounds<Pixels>> {
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

fn metrics(settings: &ViewSettings, window: &mut Window) -> Metrics {
    let font = font(crate::MONOSPACE);
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
    }
}
