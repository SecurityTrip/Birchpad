//! What is drawn over and beside the text: selections, carets, bookmarks, change bars, folds,
//! marks of search results and differences.

use std::ops::Range as ByteRange;

use birchpad_core::{LineChange, Rope};
use gpui_kit::{App, Bounds, Pixels, point, px, size};

use crate::EditorView;
use crate::theme::{self, Paint};

use super::Layout;

impl EditorView {
    /// The document bytes shown in this frame.
    pub(super) fn visible_range(layout: &Layout) -> ByteRange<usize> {
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

    pub(super) fn bookmark_symbols(
        &self,
        layout: &Layout,
        text: &Rope,
        cx: &App,
    ) -> Vec<Bounds<Pixels>> {
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
    pub(super) fn change_bars(
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
    pub(super) fn hidden_marks(&self, layout: &mut Layout, text: &Rope) {
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
    pub(super) fn fold_marks(&self, layout: &mut Layout, text: &Rope) {
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

    pub(super) fn decoration_bounds(
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
        // Clickable links, underlined; the one under the mouse framed (full box mode).
        let settings = &birchpad_ui::AppState::global(cx).settings;
        let links = &settings.links;
        let large = self.buffer.read(cx).is_large(cx) && !settings.files.large_file_links;
        if links.enabled && (links.underline || links.hover_box) && !large {
            let underline = Paint::Underline(theme::paint(theme::ui().accent));
            let boxed = Paint::Outline(theme::paint(theme::ui().accent));
            let hovered = self.hovered.filter(|_| links.hover_box);
            let underlines = links.underline;
            let extra = links.extra_schemes();
            let lines = birchpad_core::motion::line_of(text, visible.start)
                ..birchpad_core::motion::line_of(text, visible.end) + 1;
            for line in lines {
                let bounds = birchpad_core::motion::line_range(text, line);
                let content = text.slice(bounds.clone()).to_string();
                ranges.extend(
                    birchpad_core::links::links_in(&content, &extra)
                        .into_iter()
                        .map(|link| bounds.start + link.start..bounds.start + link.end)
                        .filter_map(|link| {
                            if hovered.is_some_and(|at| link.contains(&at)) {
                                Some((link, boxed))
                            } else {
                                underlines.then_some((link, underline))
                            }
                        }),
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
    pub(super) fn difference_markers(
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

    pub(super) fn selection_bounds(&mut self, layout: &Layout, text: &Rope) -> Vec<Bounds<Pixels>> {
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

    pub(super) fn caret_bounds(
        &mut self,
        layout: &Layout,
        text: &Rope,
        width: birchpad_config::CaretWidth,
    ) -> Vec<Bounds<Pixels>> {
        use birchpad_config::CaretWidth;
        let mut carets = Vec::new();
        if width == CaretWidth::None && !self.overwrite {
            return carets;
        }
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
                let cell = layout.metrics.cell;
                match width {
                    CaretWidth::Block => Bounds::new(point(x, row.y), size(cell, line_height)),
                    CaretWidth::BlockAfter => {
                        Bounds::new(point(x - cell, row.y), size(cell, line_height))
                    }
                    CaretWidth::None | CaretWidth::One => {
                        Bounds::new(point(x, row.y), size(px(1.), line_height))
                    }
                    CaretWidth::Two => Bounds::new(point(x, row.y), size(px(2.), line_height)),
                    CaretWidth::Three => Bounds::new(point(x, row.y), size(px(3.), line_height)),
                }
            });
        }
        carets
    }
}
