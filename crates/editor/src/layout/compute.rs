//! Computing a frame's layout: the visible rows, the scroll position, the margins, wrapping.

use std::ops::Range as ByteRange;

use birchpad_core::Rope;
use birchpad_core::motion::{line_count, line_range};
use birchpad_view::{DisplayMap, DisplayText, LayoutConfig, Row};
use gpui_kit::{AppContext as _, Bounds, Context, Pixels, Window, font, point, px};

use crate::symbols::Symbols;
use crate::theme::{self};
use crate::{EditorView, ViewSettings};

use super::scrollbars::{horizontal_scrollbar, vertical_scrollbar};
use super::shaping::shape;
use super::{
    BACKGROUND_WRAP, CARET_MARGIN_COLUMNS, LONG_ROW, LONG_ROW_MARGIN, Layout, Margins, Metrics,
    QUERY_GAP, SCROLLBAR, StickyRow, TEXT_PADDING, VisibleRow,
};

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

        // Preferences > Margins > Padding: room taken from the text area on each side.
        let padding = settings.padding_left + settings.padding_right;
        if text_bounds.size.width > padding + px(40.) {
            text_bounds.origin.x += settings.padding_left;
            text_bounds.size.width -= padding;
        }
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
                wrap_indent: match settings.wrap_indent {
                    birchpad_config::WrapIndent::Default => birchpad_view::WrapIndent::None,
                    birchpad_config::WrapIndent::Aligned => birchpad_view::WrapIndent::Aligned,
                    birchpad_config::WrapIndent::Indent => birchpad_view::WrapIndent::Indent,
                },
                labels: settings.labels(),
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
        // Scrolling beyond the last line: it can go up to the top.
        let max_top = if settings.scroll_beyond_last_line {
            total_rows.saturating_sub(1)
        } else {
            total_rows.saturating_sub(full_rows)
        } as f64;
        self.scroll_top = self.scroll_top.clamp(0., max_top);

        let column_zero = text_bounds.left() + TEXT_PADDING - self.scroll_left;
        let first_row = self.scroll_top.floor() as usize;
        let offset = line_height * (self.scroll_top - first_row as f64) as f32;
        let visible_count = full_rows + 2;

        // First the rows and the part of each that is shown, then their highlights, then shaping.
        // The grid the display map lays out with, which the text drawn follows.
        let grid = self.display.config().grid();
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
                    grid,
                    settings.show_control_chars,
                );
                let x = column_zero + cell * (start_column - row.start_column) as f32;
                (display, x)
            } else {
                let display = DisplayText::with_pictures(
                    &text,
                    row.range.clone(),
                    row.start_column,
                    grid,
                    settings.show_control_chars,
                );
                let x = match wrap_width {
                    // Rows after the first of a line start under its indentation (Line Wrap).
                    Some(width) if row.index_in_line > 0 => {
                        let line = birchpad_core::motion::line_range(&text, row.line);
                        let indent = birchpad_view::wrap_indent(
                            text.slice(line),
                            width,
                            settings.tab_width,
                            self.display.config().wrap_indent,
                        );
                        text_bounds.left() + TEXT_PADDING + cell * indent as f32
                    }
                    Some(_) => text_bounds.left() + TEXT_PADDING,
                    None => column_zero,
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
        let shared_before = crate::smart_highlight::shared_query();
        if self.update_smart_highlight(&shown, &text, cx) {
            // Searching continues in the next frame.
            window.request_animation_frame();
        }
        // The other view highlights what this one shares: it draws again.
        if self.focused && crate::smart_highlight::shared_query() != shared_before {
            window.refresh();
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
        let mut styles = if sticky_ranges.is_empty() {
            self.frame_styles(&shown, &text, cx)
        } else {
            self.frame_styles(&[sticky_ranges.clone(), shown.clone()].concat(), &text, cx)
        };
        if birchpad_ui::AppState::global(cx)
            .settings
            .editor
            .selection_foreground
        {
            styles.selected_color = Some(theme::editor().selection_text.0);
            styles.selected = self
                .selection
                .iter()
                .filter(|range| !range.is_empty())
                .map(|range| range.from()..range.to())
                .collect();
        }
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
            let display =
                DisplayText::with_pictures(&text, range, 0, grid, settings.show_control_chars);
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
            caret_block: false,
            border_width: settings.border_width,
            fold_style: settings.fold_style,
        };
        layout.bookmarks = self.bookmark_symbols(&layout, &text, cx);
        layout.change_bars = self.change_bars(&layout, &text, cx);
        self.fold_marks(&mut layout, &text);
        self.hidden_marks(&mut layout, &text);
        layout.decorations = self.decoration_bounds(&layout, &text, cx);
        layout.selections = self.selection_bounds(&layout, &text);
        layout.carets = self.caret_bounds(&layout, &text, settings.caret_width);
        layout.caret_block = matches!(
            settings.caret_width,
            birchpad_config::CaretWidth::Block | birchpad_config::CaretWidth::BlockAfter
        );
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
    pub(super) fn update_display_config(
        &mut self,
        text: &Rope,
        config: LayoutConfig,
        cx: &mut Context<Self>,
    ) {
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
        self.wrap_job = Some(crate::view::WrapJob {
            config,
            revision,
            _task: task,
        });
    }
}

/// The parts of the text shown by `rows`, merged where they are close: what highlighting and
/// smart highlighting look at.
pub(super) fn shown_ranges(rows: &[(Row, Pixels, Pixels, DisplayText)]) -> Vec<ByteRange<usize>> {
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

pub(super) fn metrics(settings: &ViewSettings, window: &mut Window) -> Metrics {
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
