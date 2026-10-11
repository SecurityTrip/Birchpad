//! Document Map, as Notepad++'s (View > Document Map): the whole document in miniature, with
//! the part the view shows framed.
//!
//! Each line is a strip three pixels high in which every run of characters is a bar in its
//! syntax color, a pixel and a half per column; whitespace is left blank. A map longer than
//! the panel scrolls along with the view, so that the frame keeps the same share of the way
//! down. A click or a drag on the map scrolls the view to center that place; the mouse wheel
//! scrolls the view.

use std::ops::Range;

use birchpad_core::Rope;
use birchpad_core::motion::{line_count, line_range};
use birchpad_view::cells_at;
use gpui_kit::{
    App, Bounds, Context, Entity, FocusHandle, Focusable, Hsla, MouseButton, MouseDownEvent,
    MouseMoveEvent, Pixels, ScrollWheelEvent, WeakEntity, Window, canvas, div, fill, point,
    prelude::*, px, rgb, size,
};

use super::FollowView;
use crate::editor::EditorView;
use crate::editor::theme;
use crate::workspace::Workspace;

/// Height of a line on the map.
pub(crate) const LINE_HEIGHT: f32 = 3.;
/// Height of a line's bars.
const BAR_HEIGHT: f32 = 2.;
/// Width of a column.
const COLUMN_WIDTH: f32 = 1.5;
/// The first document line the map shows: 0 while the whole document fits, else so that the
/// frame of the `visible` lines sits as far down the map as they sit in the document.
pub(crate) fn first_map_line(total: usize, visible: Range<usize>, map_lines: usize) -> usize {
    if total <= map_lines {
        return 0;
    }
    let scrollable = total - map_lines;
    let shown = visible.end.saturating_sub(visible.start).min(total);
    let movable = total.saturating_sub(shown);
    if movable == 0 {
        return 0;
    }
    (visible.start.min(movable) * scrollable / movable).min(scrollable)
}

/// The bars of one line: runs of columns that are not whitespace, each with the color of the
/// highlight over it (`spans`: byte ranges in the line, sorted), up to `max_columns`.
pub(crate) fn line_bars(
    line: &str,
    spans: &[(Range<usize>, u32)],
    tab_width: usize,
    max_columns: usize,
) -> Vec<(Range<usize>, u32)> {
    let mut bars: Vec<(Range<usize>, u32)> = Vec::new();
    let mut column = 0;
    let mut span = 0;
    for (offset, ch) in line.char_indices() {
        if column >= max_columns {
            break;
        }
        let width = cells_at(ch, column, tab_width);
        if !ch.is_whitespace() {
            while span < spans.len() && spans[span].0.end <= offset {
                span += 1;
            }
            let color = spans
                .get(span)
                .filter(|(range, _)| range.start <= offset)
                .map_or(theme_text(), |(_, color)| *color);
            let end = (column + width).min(max_columns);
            match bars.last_mut() {
                Some((last, last_color)) if last.end == column && *last_color == color => {
                    last.end = end;
                }
                _ => bars.push((column..end, color)),
            }
        }
        column += width;
    }
    bars
}

fn theme_text() -> u32 {
    theme::editor().text.to_rgb()
}

/// What the map draws for one frame.
struct MapPaint {
    first: usize,
    lines: Vec<Vec<(Range<usize>, u32)>>,
    /// The view's lines, as rows of the map from `first`.
    frame: Range<f32>,
}

pub(crate) struct DocumentMap {
    workspace: WeakEntity<Workspace>,
    follow: FollowView,
    /// Where the map was drawn, and the first line it showed.
    drawn: Option<(Bounds<Pixels>, usize)>,
    dragging: bool,
    focus_handle: FocusHandle,
}

impl Focusable for DocumentMap {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl DocumentMap {
    pub(crate) fn new(
        workspace: WeakEntity<Workspace>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            workspace,
            follow: FollowView::default(),
            drawn: None,
            dragging: false,
            focus_handle: cx.focus_handle(),
        }
    }

    fn active_view(&self, cx: &App) -> Option<Entity<EditorView>> {
        self.workspace
            .upgrade()
            .and_then(|workspace| workspace.read(cx).active_view(cx))
    }

    /// The map of `view` for a panel `height` high.
    fn paint_data(view: &Entity<EditorView>, height: Pixels, width: Pixels, cx: &App) -> MapPaint {
        let view = view.read(cx);
        let buffer = view.buffer.read(cx);
        let text: &Rope = buffer.doc().text();
        let total = line_count(text);
        let visible = view.visible_lines().unwrap_or(0..1);
        let map_lines = (f32::from(height) / LINE_HEIGHT).floor().max(1.) as usize;
        let first = first_map_line(total, visible.clone(), map_lines);
        let last = (first + map_lines).min(total);
        let max_columns = (f32::from(width) / COLUMN_WIDTH).ceil() as usize;
        let tab_width = view.tab_width();
        let start = line_range(text, first).start;
        let end = line_range(text, last.saturating_sub(1).max(first)).end;
        let language = buffer.syntax().map(|syntax| syntax.language().id);
        let highlights = buffer
            .syntax()
            .map(|syntax| syntax.highlights(text, start..end))
            .unwrap_or_default();
        let mut next = 0;
        let lines = (first..last)
            .map(|line| {
                let bounds = line_range(text, line);
                // Enough bytes for the columns shown: a column is at most a byte or a tab.
                let cut =
                    text.floor_char_boundary((bounds.start + max_columns * 4).min(bounds.end));
                let content = text.slice(bounds.start..cut).to_string();
                while next < highlights.len() && highlights[next].0.end <= bounds.start {
                    next += 1;
                }
                let spans: Vec<(Range<usize>, u32)> = highlights[next..]
                    .iter()
                    .take_while(|(range, _)| range.start < cut)
                    .filter_map(|(range, highlight)| {
                        let color = theme::syntax_style(language, *highlight)?.color? >> 8;
                        Some((
                            range.start.max(bounds.start) - bounds.start
                                ..range.end.min(cut) - bounds.start,
                            color,
                        ))
                    })
                    .collect();
                line_bars(&content, &spans, tab_width, max_columns)
            })
            .collect();
        MapPaint {
            first,
            lines,
            frame: visible.start.saturating_sub(first) as f32
                ..visible.end.saturating_sub(first) as f32,
        }
    }

    /// Scrolls the view so that the line under `y` (window coordinates) is in its middle.
    pub(crate) fn scroll_to(&mut self, y: Pixels, cx: &mut Context<Self>) {
        let (Some((bounds, first)), Some(view)) = (self.drawn, self.active_view(cx)) else {
            return;
        };
        let row = (f32::from(y - bounds.top()) / LINE_HEIGHT).max(0.) as usize;
        let line = first + row;
        view.update(cx, |view, cx| {
            let shown = view.visible_lines().map_or(1, |lines| lines.len());
            view.scroll_to_line(line.saturating_sub(shown / 2), cx);
        });
    }
}

impl Render for DocumentMap {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = self.active_view(cx);
        self.follow.follow(view.as_ref(), cx);
        let Some(view) = view else {
            return div()
                .p_2()
                .text_color(crate::theme::paint(crate::theme::ui().muted))
                .child("No document")
                .into_any_element();
        };
        let this = cx.entity().downgrade();
        let painted = view.clone();
        div()
            .id("document-map")
            .debug_selector(|| "document-map".into())
            .track_focus(&self.focus_handle)
            .size_full()
            .overflow_hidden()
            .bg(theme::paint(theme::editor().background))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.dragging = true;
                    this.scroll_to(event.position.y, cx);
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if this.dragging && event.pressed_button == Some(MouseButton::Left) {
                    this.scroll_to(event.position.y, cx);
                } else {
                    this.dragging = false;
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.dragging = false),
            )
            .on_scroll_wheel(cx.listener(move |_, event: &ScrollWheelEvent, window, cx| {
                let line_height = window.line_height();
                let rows = -f64::from(event.delta.pixel_delta(line_height).y / line_height);
                view.update(cx, |view, cx| view.scroll_by(rows, px(0.), cx));
            }))
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let data =
                            Self::paint_data(&painted, bounds.size.height, bounds.size.width, cx);
                        this.update(cx, |this, _| this.drawn = Some((bounds, data.first)))
                            .ok();
                        data
                    },
                    |bounds, data, window, _| {
                        for (row, bars) in data.lines.iter().enumerate() {
                            let y = bounds.top() + px(row as f32 * LINE_HEIGHT);
                            for (columns, color) in bars {
                                let x = bounds.left() + px(columns.start as f32 * COLUMN_WIDTH);
                                let width = px(columns.len() as f32 * COLUMN_WIDTH);
                                let color: Hsla = rgb(*color).into();
                                window.paint_quad(fill(
                                    Bounds::new(point(x, y), size(width, px(BAR_HEIGHT))),
                                    color.opacity(0.75),
                                ));
                            }
                        }
                        let top = bounds.top() + px(data.frame.start * LINE_HEIGHT);
                        let height = px((data.frame.end - data.frame.start).max(1.) * LINE_HEIGHT);
                        let frame =
                            Bounds::new(point(bounds.left(), top), size(bounds.size.width, height));
                        window.paint_quad(
                            fill(frame, theme::paint(theme::ui().accent.with_alpha(0x22)))
                                .border_widths(px(1.))
                                .border_color(theme::paint(theme::ui().accent.with_alpha(0x88))),
                        );
                    },
                )
                .size_full(),
            )
            .into_any_element()
    }
}

#[cfg(test)]
#[path = "document_map_tests.rs"]
mod tests;
