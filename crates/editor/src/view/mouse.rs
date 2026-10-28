//! The mouse: scrolling, clicks, drags of text and scrollbars, the context menu.

use std::ops::Range as ByteRange;

use birchpad_core::motion::{self, line_of, line_range, line_range_with_break};
use birchpad_core::{Edit, Range, Rope, Selection, Transaction};
use birchpad_ui::AppState;
use birchpad_view::{Block, BlockPoint};
use gpui_kit::{
    App, Context, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
    ScrollWheelEvent, Window, point, px,
};

use crate::ZOOM_RANGE;

use super::{Drag, EditorView, Granularity, LastEdit};

impl EditorView {
    // --- Scrolling -----------------------------------------------------------------------------

    pub(crate) fn scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(layout) = self.layout.as_ref() else {
            return;
        };
        let line_height = layout.metrics.line_height;
        if event.modifiers.secondary() {
            // Ctrl+wheel zooms, as in Notepad++.
            let delta = event.delta.pixel_delta(line_height).y;
            if delta != px(0.) {
                let step = if delta > px(0.) { 1 } else { -1 };
                AppState::update_state(cx, |state, _| {
                    state.zoom = (state.zoom + step).clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end());
                });
                self.settings_changed(cx);
                cx.refresh_windows();
            }
            return;
        }
        let mut delta = event.delta.pixel_delta(line_height);
        if event.modifiers.shift && delta.x == px(0.) {
            delta = point(delta.y, px(0.));
        }
        self.scroll_top = (self.scroll_top - f64::from(delta.y / line_height)).max(0.);
        self.scroll_left = (self.scroll_left - delta.x).max(px(0.));
        cx.notify();
    }

    // --- Mouse ---------------------------------------------------------------------------------

    pub(crate) fn with_primary(&self, range: Range) -> Selection {
        let mut ranges = self.selection.ranges().to_vec();
        let primary = self.selection.primary_index();
        ranges[primary] = range;
        Selection::new(ranges, primary)
    }

    /// The range a click at `pos` selects at `granularity`.
    pub(crate) fn unit_at(text: &Rope, pos: usize, granularity: Granularity) -> ByteRange<usize> {
        match granularity {
            Granularity::Char => pos..pos,
            Granularity::Word => motion::word_at(text, pos),
            Granularity::Line => line_range_with_break(text, line_of(text, pos)),
        }
    }

    /// A Ctrl+click on a link opens it, and a double click too with `links.double-click`;
    /// Ctrl+double click selects between the delimiters around the click. Whether the click was
    /// taken.
    pub(crate) fn link_or_delimiters(
        &mut self,
        event: &MouseDownEvent,
        pos: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        let settings = AppState::global(cx).settings.clone();
        let secondary = event.modifiers.secondary();
        let text = self.text(cx).clone();
        let opens = (event.click_count == 1 && secondary)
            || (event.click_count == 2 && !secondary && settings.links.double_click);
        if settings.links.enabled
            && opens
            && let Some(link) =
                birchpad_core::links::link_at(&text, pos, &settings.links.extra_schemes())
        {
            let url = text.slice(link).to_string();
            (crate::EditorHooks::get(cx).open_link)(url, cx);
            self.drag = None;
            return true;
        }
        if secondary
            && event.click_count == 2
            && let Some((open, close)) = settings.delimiters.pair()
            && let Some(range) = birchpad_core::links::between_delimiters(
                &text,
                pos,
                open,
                close,
                settings.delimiters.multi_line,
            )
        {
            self.selection = Selection::single(Range::new(range.start, range.end));
            self.drag = None;
            self.goal_column = None;
            cx.notify();
            return true;
        }
        false
    }

    /// A right click: the caret goes there, unless the click is in a selection (the context
    /// menu then acts on it).
    pub(crate) fn right_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        self.completion = None;
        self.call_tip = None;
        let Some(pos) = self.position_for_point(event.position, cx) else {
            return;
        };
        let in_selection = self
            .selection
            .iter()
            .any(|range| !range.is_empty() && range.from() <= pos && pos <= range.to());
        // Preferences > Editing > Keep selection when right-click outside of selection.
        let keep = AppState::global(cx)
            .settings
            .editor
            .keep_selection_on_right_click
            && self.selection.iter().any(|range| !range.is_empty());
        if !in_selection && !keep {
            self.go_to(pos, cx);
        }
    }

    pub(crate) fn mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        self.completion = None;
        self.call_tip = None;
        let Some(layout) = self.layout.as_ref() else {
            return;
        };
        if let Some(bar) = &layout.vertical_bar
            && bar.track.contains(&event.position)
        {
            let grab = if bar.thumb.contains(&event.position) {
                event.position.y - bar.thumb.top()
            } else {
                bar.thumb.size.height / 2.
            };
            self.drag = Some(Drag::VerticalThumb { grab });
            self.drag_scrollbar(event.position, cx);
            return;
        }
        if let Some(bar) = &layout.horizontal_bar
            && bar.track.contains(&event.position)
        {
            let grab = if bar.thumb.contains(&event.position) {
                event.position.x - bar.thumb.left()
            } else {
                bar.thumb.size.width / 2.
            };
            self.drag = Some(Drag::HorizontalThumb { grab });
            self.drag_scrollbar(event.position, cx);
            return;
        }
        // A click on a line sticky scroll keeps goes there, shown under the lines above it.
        if let Some(line) = layout.sticky_at(event.position.y)
            && (layout.text_bounds.contains(&event.position)
                || layout.margins.gutter.contains(&event.position))
        {
            let above = layout
                .sticky
                .iter()
                .position(|row| row.line == line)
                .unwrap_or(0);
            let text = self.text(cx).clone();
            self.go_to(birchpad_core::motion::line_range(&text, line).start, cx);
            self.scroll_to_line(line, cx);
            self.scroll_top = (self.scroll_top - above as f64).max(0.);
            self.autoscroll = false;
            return;
        }
        // A click in the margins of a line with hidden lines below shows them again.
        if layout.margins.gutter.contains(&event.position)
            && let Some(line) = layout
                .row_at_y(event.position.y)
                .map(|row| row.row.line)
                .filter(|line| layout.hidden_marks.iter().any(|(anchor, _)| anchor == line))
        {
            self.show_hidden_below(line, cx);
            return;
        }
        if let Some(margin) = layout.margins.folding
            && margin.contains(&event.position)
        {
            // A click on a fold point toggles it.
            let header = layout
                .row_at_y(event.position.y)
                .filter(|row| row.row.index_in_line == 0)
                .map(|row| row.row.line);
            if let Some(line) = header {
                self.toggle_fold(line, cx);
            }
            return;
        }
        if let Some(margin) = layout.margins.symbols
            && margin.contains(&event.position)
        {
            // A click in the symbol margin toggles the bookmark, as in Notepad++.
            if let Some(line) = layout.row_at_y(event.position.y).map(|row| row.row.line) {
                self.buffer.update(cx, |buffer, cx| {
                    buffer.update_marks(cx, |marks, text| {
                        marks.bookmarks.toggle(text, line);
                    });
                });
            }
            return;
        }
        // In the line number margin, clicking and dragging selects whole lines; in the change
        // history margin too, as in Scintilla's margins that take no clicks of their own.
        let in_line_numbers = [layout.margins.line_numbers, layout.margins.changes]
            .into_iter()
            .flatten()
            .any(|margin| margin.contains(&event.position));
        let text = self.text(cx).clone();
        let Some(pos) = self.position_for_point(event.position, cx) else {
            return;
        };
        let granularity = match event.click_count {
            _ if in_line_numbers => Granularity::Line,
            0 | 1 => Granularity::Char,
            2 => Granularity::Word,
            _ => Granularity::Line,
        };
        if event.modifiers.alt
            && !event.modifiers.shift
            && !event.modifiers.secondary()
            && granularity == Granularity::Char
            && let Some(anchor) = self.block_point_for_point(event.position, cx)
        {
            // Alt+drag selects a rectangle.
            self.set_block(Block::new(anchor, anchor), cx);
            self.drag = Some(Drag::Block { anchor });
            return;
        }
        if !in_line_numbers && !event.modifiers.shift && self.link_or_delimiters(event, pos, cx) {
            return;
        }
        let editor_settings = AppState::global(cx).settings.editor.clone();
        // A press in the selection may start dragging it elsewhere.
        if editor_settings.drag_text
            && granularity == Granularity::Char
            && !event.modifiers.shift
            && !event.modifiers.alt
            && !in_line_numbers
            && self
                .selection
                .iter()
                .any(|range| !range.is_empty() && range.from() < pos && pos < range.to())
        {
            self.drag = Some(Drag::Text { from: pos, to: pos });
            cx.notify();
            return;
        }
        let unit = Self::unit_at(&text, pos, granularity);
        self.selection = if event.modifiers.shift {
            self.with_primary(Range::new(self.selection.primary().anchor, pos))
        } else if event.modifiers.secondary()
            && granularity == Granularity::Char
            && editor_settings.multi_editing
        {
            self.selection.clone().push(Range::point(pos))
        } else if editor_settings.virtual_space
            && granularity == Granularity::Char
            && let Some(point) = self.block_point_for_point(event.position, cx)
            && let line_end = line_range(&text, point.line).end
            && let end_column = self.display.column(&text, line_end)
            && point.column > end_column
        {
            // Virtual space: past the end of the line, the caret stays where it was put.
            let past = point.column - end_column;
            Selection::single(Range::point(line_end).with_virtual(past, past))
        } else {
            Selection::single(Range::new(unit.start, unit.end))
        };
        self.drag = Some(Drag::Select {
            granularity,
            origin: unit,
        });
        self.goal_column = None;
        self.last_edit = LastEdit::None;
        self.pause_blink(cx);
        cx.notify();
    }

    pub fn mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = self.drag.clone() else {
            // Cloud & Link > full box mode: the link under the mouse is framed.
            if event.pressed_button.is_none() && AppState::global(cx).settings.links.hover_box {
                let hovered = self.position_for_point(event.position, cx);
                if hovered != self.hovered {
                    self.hovered = hovered;
                    cx.notify();
                }
            }
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.drag = None;
            return;
        }
        // Dragging past the top or bottom scrolls.
        if matches!(drag, Drag::Select { .. } | Drag::Block { .. })
            && let Some(layout) = &self.layout
        {
            if event.position.y < layout.text_bounds.top() {
                self.scroll_top = (self.scroll_top - 1.).max(0.);
            } else if event.position.y > layout.text_bounds.bottom() {
                self.scroll_top += 1.;
            }
        }
        if let Drag::Block { anchor } = drag {
            if let Some(head) = self.block_point_for_point(event.position, cx) {
                self.set_block(Block::new(anchor, head), cx);
            }
            return;
        }
        if let Drag::Text { from, .. } = drag {
            if let Some(to) = self.position_for_point(event.position, cx) {
                self.drag = Some(Drag::Text { from, to });
                cx.notify();
            }
            return;
        }
        let Drag::Select {
            granularity,
            origin,
        } = drag
        else {
            self.drag_scrollbar(event.position, cx);
            return;
        };
        let text = self.text(cx).clone();
        let Some(pos) = self.position_for_point(event.position, cx) else {
            return;
        };
        let unit = Self::unit_at(&text, pos, granularity);
        let range = if pos < origin.start {
            Range::new(origin.end, unit.start)
        } else {
            Range::new(origin.start, unit.end.max(origin.end))
        };
        self.selection = self.with_primary(range);
        // Dragging past the left or right edge scrolls the caret into view, a few columns at a
        // time, so the selection goes on past the visible part of a long line.
        if let Some(layout) = &self.layout
            && (event.position.x < layout.text_bounds.left()
                || event.position.x > layout.text_bounds.right())
        {
            self.autoscroll = true;
        }
        self.pause_blink(cx);
        cx.notify();
    }

    pub(crate) fn mouse_up(
        &mut self,
        event: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(Drag::Text { from, to }) = self.drag.take() {
            if from == to {
                // A click in the selection without dragging puts the caret there.
                self.go_to(to, cx);
            } else {
                self.drop_selection(to, event.modifiers.secondary(), cx);
            }
        }
        self.drag = None;
    }

    /// The selected text dragged to `to`: moved there, or copied with Ctrl, as one undo step.
    /// Dropped inside the selection, nothing changes.
    pub fn drop_selection(&mut self, to: usize, copy: bool, cx: &mut Context<Self>) {
        let primary = self.selection.primary();
        let range = primary.from()..primary.to();
        if range.is_empty() || !self.is_editable(cx) || (range.start <= to && to <= range.end) {
            return;
        }
        let text = self.text(cx).clone();
        let moved = text.slice(range.clone()).to_string();
        let mut edits = vec![Edit::replace(to..to, moved.clone())];
        if !copy {
            edits.push(Edit::replace(range.clone(), String::new()));
        }
        edits.sort_by_key(|edit| edit.range.start);
        let Ok(transaction) = Transaction::from_edits(&text, edits) else {
            return;
        };
        // The text where it landed is selected.
        let start = if !copy && to > range.end {
            to - range.len()
        } else {
            to
        };
        let selection = Selection::single(Range::new(start, start + moved.len()));
        self.apply(transaction.with_selection(selection), LastEdit::None, cx);
    }

    pub(crate) fn drag_scrollbar(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(layout) = self.layout.as_ref() else {
            return;
        };
        match self.drag {
            Some(Drag::VerticalThumb { grab }) => {
                if let Some(bar) = &layout.vertical_bar {
                    let free = bar.track.size.height - bar.thumb.size.height;
                    let fraction = if free > px(0.) {
                        ((position.y - grab - bar.track.top()) / free).clamp(0., 1.)
                    } else {
                        0.
                    };
                    let max = layout.total_rows.saturating_sub(layout.full_rows) as f64;
                    self.scroll_top = f64::from(fraction) * max;
                }
            }
            Some(Drag::HorizontalThumb { grab }) => {
                if let Some(bar) = &layout.horizontal_bar {
                    let free = bar.track.size.width - bar.thumb.size.width;
                    let fraction = if free > px(0.) {
                        ((position.x - grab - bar.track.left()) / free).clamp(0., 1.)
                    } else {
                        0.
                    };
                    let max = (self.scroll_width - layout.text_bounds.size.width).max(px(0.));
                    self.scroll_left = max * fraction;
                }
            }
            _ => {}
        }
        cx.notify();
    }

    /// The text position under a window point, using the last frame's layout.
    pub(crate) fn position_for_point(&mut self, point: Point<Pixels>, cx: &App) -> Option<usize> {
        let layout = self.layout.as_ref()?;
        let text = self.buffer.read(cx).doc().text();
        layout.position_for_point(point, text, &mut self.display)
    }

    /// The rectangle corner under a window point: past the end of a line, in virtual space.
    pub(crate) fn block_point_for_point(
        &mut self,
        point: Point<Pixels>,
        cx: &App,
    ) -> Option<BlockPoint> {
        let layout = self.layout.as_ref()?;
        let text = self.buffer.read(cx).doc().text();
        layout.block_point_for_point(point, text, &mut self.display)
    }
}
