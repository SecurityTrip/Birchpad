//! Paints one line of the document: text, selection backgrounds, IME underline and carets.

use gpui_kit::{
    App, Bounds, Element, ElementId, Entity, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, PaintQuad, Pixels, ShapedLine, Style, TextAlign, TextRun, UnderlineStyle, Window,
    fill, point, px, relative, rgb, rgba, size,
};

use crate::editor::{Editor, VisibleLine};

const SELECTION: u32 = 0x3390ff40;
const CARET: u32 = 0x1f2328;

pub(crate) struct LineElement {
    editor: Entity<Editor>,
    line: usize,
}

impl LineElement {
    pub(crate) fn new(editor: Entity<Editor>, line: usize) -> Self {
        Self { editor, line }
    }
}

pub(crate) struct Prepared {
    shaped: ShapedLine,
    start: usize,
    quads: Vec<PaintQuad>,
    carets: Vec<PaintQuad>,
}

impl IntoElement for LineElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for LineElement {
    type RequestLayoutState = ();
    type PrepaintState = Prepared;

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
        style.size.height = window.line_height().into();
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
        let editor = self.editor.read(cx);
        let range = editor.line_range(self.line);
        // Tabs are drawn as single spaces for now: one byte each, so offsets stay valid.
        let content = editor
            .doc
            .text()
            .slice(range.clone())
            .to_string()
            .replace('\t', " ");

        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let base = TextRun {
            len: content.len(),
            font: style.font(),
            color: style.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };

        // Split the line wherever a selection or the IME composition starts or ends.
        let clip = |from: usize, to: usize| {
            let local = |pos: usize| pos.clamp(range.start, range.end) - range.start;
            (from < to && from < range.end && to > range.start).then(|| local(from)..local(to))
        };
        let mut cuts = vec![0, content.len()];
        let spans = editor
            .selection
            .iter()
            .map(|r| (r.from(), r.to()))
            .chain(editor.marked.iter().map(|m| (m.start, m.end)));
        for (from, to) in spans {
            if let Some(local) = clip(from, to) {
                cuts.extend([local.start, local.end]);
            }
        }
        cuts.sort_unstable();
        cuts.dedup();

        let mut runs: Vec<TextRun> = cuts
            .windows(2)
            .map(|cut| {
                let pos = range.start + cut[0];
                let selected = editor
                    .selection
                    .iter()
                    .any(|r| r.from() <= pos && pos < r.to());
                let marked = editor.marked.as_ref().is_some_and(|m| m.contains(&pos));
                TextRun {
                    len: cut[1] - cut[0],
                    background_color: selected.then(|| rgba(SELECTION).into()),
                    underline: marked.then(|| UnderlineStyle {
                        color: Some(style.color),
                        thickness: px(1.),
                        wavy: false,
                    }),
                    ..base.clone()
                }
            })
            .collect();
        if runs.is_empty() {
            runs.push(base);
        }
        let shaped = window
            .text_system()
            .shape_line(content.into(), font_size, &runs, None);

        let height = bounds.size.height;
        let mut quads = Vec::new();
        let mut carets = Vec::new();
        for selection in editor.selection.iter() {
            // A selected line break is shown as a small block after the text.
            if selection.from() <= range.end && range.end < selection.to() {
                let origin = point(bounds.left() + shaped.width(), bounds.top());
                quads.push(fill(
                    Bounds::new(origin, size(px(7.), height)),
                    rgba(SELECTION),
                ));
            }
            if editor.line_of(selection.head) == self.line {
                let x = shaped.x_for_index(selection.head - range.start);
                let origin = point(bounds.left() + x, bounds.top());
                carets.push(fill(Bounds::new(origin, size(px(2.), height)), rgb(CARET)));
            }
        }

        Prepared {
            shaped,
            start: range.start,
            quads,
            carets,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepared: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let line_height = window.line_height();
        for quad in prepared.quads.drain(..) {
            window.paint_quad(quad);
        }
        let shaped = &prepared.shaped;
        shaped
            .paint_background(
                bounds.origin,
                line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            )
            .ok();
        shaped
            .paint(
                bounds.origin,
                line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            )
            .ok();

        if self.editor.read(cx).focus_handle.is_focused(window) {
            for caret in prepared.carets.drain(..) {
                window.paint_quad(caret);
            }
        }

        let visible = VisibleLine {
            bounds,
            shaped: prepared.shaped.clone(),
            start: prepared.start,
        };
        let line = self.line;
        self.editor.update(cx, |editor, _| {
            editor.visible_lines.insert(line, visible);
        });
    }
}
