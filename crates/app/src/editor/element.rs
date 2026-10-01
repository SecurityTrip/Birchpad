//! The element that paints an editor view: gutter, selections, text, carets, scrollbars.

use gpui_kit::{
    App, Bounds, ContentMask, CursorStyle, Element, ElementId, ElementInputHandler, Entity,
    GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId, IntoElement, LayoutId, Pixels,
    Style, TextAlign, Window, fill, relative, rgb, rgba,
};

use super::EditorView;

const GUTTER_BACKGROUND: u32 = 0xf6f8fa;
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

        window.paint_quad(fill(layout.gutter, rgb(GUTTER_BACKGROUND)));
        window.with_content_mask(
            Some(ContentMask {
                bounds: layout.gutter,
            }),
            |window| {
                for (origin, number) in &layout.line_numbers {
                    number
                        .paint(*origin, line_height, TextAlign::Left, None, window, cx)
                        .ok();
                }
            },
        );
        window.with_content_mask(
            Some(ContentMask {
                bounds: layout.text_bounds,
            }),
            |window| {
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
