//! The element that paints an editor view: margins, decorations, selections, text, carets,
//! scrollbars.

use gpui_kit::{
    App, Bounds, ContentMask, CursorStyle, Element, ElementId, ElementInputHandler, Entity,
    GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId, IntoElement, LayoutId, Pixels,
    Style, TextAlign, Window, fill, outline, point, px, relative, rgb, rgba, size,
};

use super::EditorView;
use super::theme::{self, Paint};

const SELECTION: u32 = 0x3390ff40;
const CARET: u32 = 0x1f2328;
const SCROLLBAR_TRACK: u32 = 0xf6f8fa;
const SCROLLBAR_THUMB: u32 = 0xc4c9cf;

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
        window.paint_quad(fill(gutter, rgb(theme::GUTTER_BACKGROUND)));
        if gutter.size.width > px(0.) {
            window.paint_quad(fill(
                Bounds::new(
                    point(gutter.right() - px(1.), gutter.top()),
                    size(px(1.), gutter.size.height),
                ),
                rgb(theme::GUTTER_BORDER),
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
                    fill(*symbol, rgb(theme::BOOKMARK)).corner_radii(symbol.size.width / 2.),
                );
            }
            for line in &layout.fold_lines {
                window.paint_quad(fill(*line, rgb(theme::FOLD_LINE)));
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
                    window.paint_quad(fill(*underline, rgb(theme::FOLD_UNDERLINE)));
                }
                for selection in &layout.selections {
                    window.paint_quad(fill(*selection, rgba(SELECTION)));
                }
                for row in &layout.rows {
                    let origin = gpui_kit::point(row.x, row.y);
                    row.shaped
                        .paint(origin, line_height, TextAlign::Left, None, window, cx)
                        .ok();
                }
                if show_carets {
                    for caret in &layout.carets {
                        window.paint_quad(fill(*caret, rgb(CARET)));
                    }
                }
            },
        );
        for bar in [&layout.vertical_bar, &layout.horizontal_bar]
            .into_iter()
            .flatten()
        {
            window.paint_quad(fill(bar.track, rgb(SCROLLBAR_TRACK)));
            window.paint_quad(fill(bar.thumb, rgb(SCROLLBAR_THUMB)).corner_radii(gpui_kit::px(4.)));
        }

        self.view.update(cx, |view, _| view.layout = Some(layout));
    }
}

/// A fold box: a square with a minus (expanded) or a plus (collapsed), as Notepad++ draws them.
fn paint_fold_box(bounds: Bounds<Pixels>, collapsed: bool, window: &mut Window) {
    let mark = rgb(theme::FOLD_MARK);
    window.paint_quad(fill(bounds, rgb(0xffffff)));
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
