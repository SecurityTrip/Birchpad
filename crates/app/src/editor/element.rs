//! The element that paints an editor view: margins, decorations, selections, text, carets,
//! scrollbars.

use birchpad_core::LineChange;
use gpui_kit::{
    App, Bounds, ContentMask, CursorStyle, Element, ElementId, ElementInputHandler, Entity,
    GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId, IntoElement, LayoutId, Pixels,
    Point, Style, TextAlign, Window, fill, outline, point, px, relative, size,
};

use super::EditorView;
use super::theme::{self, Paint};

pub(super) struct EditorElement {
    view: Entity<EditorView>,
}

impl EditorElement {
    pub(super) fn new(view: Entity<EditorView>) -> Self {
        Self { view }
    }
}

impl IntoElement for EditorElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = Option<Hitbox>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let started = std::time::Instant::now();
        self.view
            .update(cx, |view, cx| view.compute_layout(bounds, window, cx));
        if std::env::var_os("BIRCHPAD_TIMINGS").is_some() {
            eprintln!("timing: layout in {:?}", started.elapsed());
        }
        let text_bounds = self.view.read(cx).layout.as_ref()?.text_bounds;
        Some(window.insert_hitbox(text_bounds, HitboxBehavior::Normal))
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.view.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.view.clone()),
            cx,
        );
        if let Some(hitbox) = hitbox {
            window.set_cursor_style(CursorStyle::IBeam, hitbox);
        }
        let show_carets = focus_handle.is_focused(window) && self.view.read(cx).caret_visible;
        // Take the layout out of the view so that painting can borrow the app mutably.
        let Some(layout) = self.view.update(cx, |view, _| view.layout.take()) else {
            return;
        };
        let line_height = layout.metrics.line_height;

        let gutter = layout.margins.gutter;
        window.paint_quad(fill(
            gutter,
            theme::paint(theme::editor().gutter_background),
        ));
        if gutter.size.width > px(0.) {
            window.paint_quad(fill(
                Bounds::new(
                    point(gutter.right() - px(1.), gutter.top()),
                    size(px(1.), gutter.size.height),
                ),
                theme::paint(theme::editor().gutter_border),
            ));
        }
        window.with_content_mask(Some(ContentMask { bounds: gutter }), |window| {
            for (origin, number) in &layout.line_numbers {
                number
                    .paint(*origin, line_height, TextAlign::Left, None, window, cx)
                    .ok();
            }
            for symbol in &layout.bookmarks {
                window.paint_quad(
                    fill(*symbol, theme::paint(theme::editor().bookmark))
                        .corner_radii(symbol.size.width / 2.),
                );
            }
            for (bar, change) in &layout.change_bars {
                let color = match change {
                    LineChange::Modified => theme::editor().change_modified,
                    LineChange::Saved => theme::editor().change_saved,
                };
                window.paint_quad(fill(*bar, theme::paint(color)));
            }
            for line in &layout.fold_lines {
                window.paint_quad(fill(*line, theme::paint(theme::editor().fold_line)));
            }
            for (fold_box, collapsed) in &layout.fold_boxes {
                paint_fold_box(*fold_box, *collapsed, window);
            }
        });
        window.with_content_mask(
            Some(ContentMask {
                bounds: layout.text_bounds,
            }),
            |window| {
                let symbols = &layout.symbols;
                if let Some(line) = symbols.current_line
                    && symbols.current_line_frame.is_none()
                {
                    window.paint_quad(fill(line, theme::paint(theme::editor().current_line)));
                }
                for background in &symbols.edge_backgrounds {
                    window.paint_quad(fill(*background, theme::paint(theme::editor().edge)));
                }
                for (bounds, paint) in &layout.decorations {
                    match *paint {
                        Paint::Fill(color) => {
                            window.paint_quad(fill(*bounds, color).corner_radii(px(2.)));
                        }
                        Paint::Outline(color) => {
                            window.paint_quad(outline(
                                *bounds,
                                color,
                                gpui_kit::BorderStyle::Solid,
                            ));
                        }
                        Paint::Underline(color) => {
                            let line = Bounds::new(
                                point(bounds.left(), bounds.bottom() - px(2.)),
                                size(bounds.size.width, px(1.)),
                            );
                            window.paint_quad(fill(line, color));
                        }
                    }
                }
                for underline in &layout.fold_underlines {
                    window.paint_quad(fill(
                        *underline,
                        theme::paint(theme::editor().fold_underline),
                    ));
                }
                for selection in &layout.selections {
                    window.paint_quad(fill(*selection, theme::paint(theme::editor().selection)));
                }
                for guide in &symbols.indent_guides {
                    paint_dotted_line(*guide, window);
                }
                for line in &symbols.edge_lines {
                    window.paint_quad(fill(*line, theme::paint(theme::editor().edge)));
                }
                for dot in &symbols.spaces {
                    window.paint_quad(fill(*dot, theme::paint(theme::editor().whitespace)));
                }
                for tab in &symbols.tabs {
                    paint_tab_arrow(*tab, window);
                }
                for row in &layout.rows {
                    let origin = gpui_kit::point(row.x, row.y);
                    row.shaped
                        .paint(origin, line_height, TextAlign::Left, None, window, cx)
                        .ok();
                }
                for (bounds, label) in &symbols.line_breaks {
                    window.paint_quad(
                        fill(*bounds, theme::paint(theme::editor().eol_box)).corner_radii(px(3.)),
                    );
                    let x = bounds.left() + (bounds.size.width - label.width()) / 2.;
                    label
                        .paint(
                            point(x, bounds.top()),
                            bounds.size.height,
                            TextAlign::Left,
                            None,
                            window,
                            cx,
                        )
                        .ok();
                }
                for mark in &symbols.wrap_marks {
                    paint_wrap_mark(*mark, window);
                }
                if let (Some(line), Some(width)) =
                    (symbols.current_line, symbols.current_line_frame)
                {
                    window.paint_quad(
                        outline(
                            line,
                            theme::paint(theme::editor().current_line),
                            gpui_kit::BorderStyle::Solid,
                        )
                        .border_widths(width),
                    );
                }
                if show_carets {
                    for caret in &layout.carets {
                        window.paint_quad(fill(*caret, theme::paint(theme::editor().caret)));
                    }
                }
            },
        );
        for bar in [&layout.vertical_bar, &layout.horizontal_bar]
            .into_iter()
            .flatten()
        {
            window.paint_quad(fill(bar.track, theme::paint(theme::ui().surface)));
            window.paint_quad(
                fill(bar.thumb, theme::paint(theme::editor().scrollbar_thumb))
                    .corner_radii(gpui_kit::px(4.)),
            );
        }

        self.view.update(cx, |view, _| view.layout = Some(layout));
    }
}

/// A fold box: a square with a minus (expanded) or a plus (collapsed), as Notepad++ draws them.
fn paint_fold_box(bounds: Bounds<Pixels>, collapsed: bool, window: &mut Window) {
    let mark = theme::paint(theme::editor().fold_mark);
    window.paint_quad(fill(bounds, theme::paint(theme::editor().background)));
    window.paint_quad(outline(bounds, mark, gpui_kit::BorderStyle::Solid));
    let inset = (bounds.size.width / 4.).round().max(px(2.));
    let middle_y = (bounds.top() + bounds.size.height / 2.).floor();
    let middle_x = (bounds.left() + bounds.size.width / 2.).floor();
    window.paint_quad(fill(
        Bounds::from_corners(
            point(bounds.left() + inset, middle_y),
            point(bounds.right() - inset, middle_y + px(1.)),
        ),
        mark,
    ));
    if collapsed {
        window.paint_quad(fill(
            Bounds::from_corners(
                point(middle_x, bounds.top() + inset),
                point(middle_x + px(1.), bounds.bottom() - inset),
            ),
            mark,
        ));
    }
}

/// One pixel wide, dotted every other pixel like Scintilla's indentation guides.
fn paint_dotted_line(bounds: Bounds<Pixels>, window: &mut Window) {
    let color = theme::paint(theme::editor().indent_guide);
    let mut y = bounds.top();
    while y < bounds.bottom() {
        window.paint_quad(fill(
            Bounds::new(point(bounds.left(), y), size(px(1.), px(1.))),
            color,
        ));
        y += px(2.);
    }
}

/// A pixel at each step of a diagonal from `from`, `steps` long, going by `dx` and `dy`.
fn paint_diagonal(from: Point<Pixels>, steps: i32, dx: f32, dy: f32, window: &mut Window) {
    for step in 0..=steps {
        let step = step as f32;
        let at = point(from.x + px(dx * step), from.y + px(dy * step));
        window.paint_quad(fill(
            Bounds::new(at, size(px(1.), px(1.))),
            theme::paint(theme::editor().whitespace),
        ));
    }
}

/// A tab as Scintilla's long arrow: a line across its cells ending in an arrowhead.
fn paint_tab_arrow(bounds: Bounds<Pixels>, window: &mut Window) {
    let color = theme::paint(theme::editor().whitespace);
    let middle = (bounds.top() + bounds.size.height / 2.).floor();
    let left = (bounds.left() + px(2.)).round();
    let right = (bounds.right() - px(2.)).round();
    if right <= left {
        return;
    }
    window.paint_quad(fill(
        Bounds::from_corners(point(left, middle), point(right, middle + px(1.))),
        color,
    ));
    let head = ((bounds.size.height * 0.2).round() / px(1.))
        .min((right - left) / px(2.))
        .floor() as i32;
    let tip = point(right - px(1.), middle);
    paint_diagonal(tip, head, -1., -1., window);
    paint_diagonal(tip, head, -1., 1., window);
}

/// The wrap symbol: a line down the right of the cell, turning left into an arrowhead (↵).
fn paint_wrap_mark(bounds: Bounds<Pixels>, window: &mut Window) {
    let color = theme::paint(theme::editor().whitespace);
    let height = bounds.size.height;
    let left = (bounds.left() + px(1.)).round();
    let right = (bounds.right() - px(2.)).round();
    let top = (bounds.top() + height * 0.25).round();
    let bottom = (bounds.top() + height * 0.65).round();
    if right <= left {
        return;
    }
    window.paint_quad(fill(
        Bounds::from_corners(point(right, top), point(right + px(1.), bottom + px(1.))),
        color,
    ));
    window.paint_quad(fill(
        Bounds::from_corners(point(left, bottom), point(right, bottom + px(1.))),
        color,
    ));
    let head = ((height * 0.15).round() / px(1.)).max(1.) as i32;
    paint_diagonal(point(left, bottom), head, 1., -1., window);
    paint_diagonal(point(left, bottom), head, 1., 1., window);
}
