//! Shaping the visible rows: their colors from highlighting and brackets, right-to-left rows.

use std::ops::Range as ByteRange;

use birchpad_core::Rope;
use birchpad_view::DisplayText;
use gpui_kit::{
    App, FontStyle, FontWeight, Hsla, Pixels, ShapedLine, TextRun, UnderlineStyle, Window, px, rgba,
};

use crate::EditorView;
use crate::theme::{self, TextStyle};

use super::{HighlightCache, Layout, Metrics, VisibleRow};

/// How the text of a frame is colored: syntax highlights and the brackets at the caret.
#[derive(Default)]
pub(super) struct FrameStyles {
    /// Sorted, non-overlapping.
    spans: Vec<(ByteRange<usize>, TextStyle)>,
    braces: Vec<(ByteRange<usize>, TextStyle)>,
    /// The selections, and the color of selected text over every other (Preferences >
    /// Editing > Selected text's own color).
    pub(super) selected: Vec<ByteRange<usize>>,
    pub(super) selected_color: Option<u32>,
}

impl FrameStyles {
    /// The style at document position `pos`: a bracket's, else the highlight's; in a
    /// selection, with the selected text's color.
    fn at(&self, pos: usize) -> Option<TextStyle> {
        let style = self.base_at(pos);
        match self.selected_color {
            Some(color) if self.selected.iter().any(|range| range.contains(&pos)) => {
                Some(TextStyle {
                    color: Some(color),
                    ..style.unwrap_or_default()
                })
            }
            _ => style,
        }
    }

    fn base_at(&self, pos: usize) -> Option<TextStyle> {
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
            .map(|(r, _)| r)
            .chain(
                self.selected
                    .iter()
                    .filter(|_| self.selected_color.is_some()),
            )
            .flat_map(|r| [r.start, r.end])
    }
}

impl EditorView {
    /// Highlights and bracket styles for the text shown in this frame (`ranges`, from
    /// [`shown_ranges`]).
    pub(super) fn frame_styles(
        &mut self,
        ranges: &[ByteRange<usize>],
        text: &Rope,
        cx: &App,
    ) -> FrameStyles {
        let buffer = self.buffer.read(cx);
        let mut styles = FrameStyles::default();
        let braces = !buffer.is_large(cx)
            || birchpad_ui::AppState::global(cx)
                .settings
                .files
                .large_file_brace_match;
        if braces
            && let Some(found) = birchpad_syntax::matching_bracket(
                text,
                buffer.syntax(),
                self.selection.primary().head,
            )
        {
            match found.partner {
                Some(partner) => {
                    styles.braces.push((found.bracket, theme::brace_match()));
                    styles.braces.push((partner, theme::brace_match()));
                }
                None => styles.braces.push((found.bracket, theme::brace_bad())),
            }
        }
        // Preferences > Highlighting > Highlight Matching Tags, in HTML and XML.
        let highlighting = &birchpad_ui::AppState::global(cx).settings.highlighting;
        let language = buffer.language().map(|language| language.id);
        if highlighting.matching_tags
            && language.is_some_and(|id| crate::completion::tag_language(id).is_some())
        {
            let head = self.selection.primary().head;
            let revision = buffer.doc().revision();
            let in_comments = highlighting.matching_tags_in_comments;
            let attributes = highlighting.matching_tag_attributes;
            let cached = self.tag_match.as_ref().filter(|(known, at, comments, _)| {
                *known == revision && *at == head && *comments == in_comments
            });
            let pair = match cached {
                Some((.., pair)) => pair.clone(),
                None => {
                    let pair = birchpad_core::ops::matching_tag(text, head, in_comments);
                    self.tag_match = Some((revision, head, in_comments, pair.clone()));
                    pair
                }
            };
            if let Some(pair) = pair {
                for tag in [pair.open, pair.close] {
                    let range = if attributes { tag.range } else { tag.name };
                    styles.braces.push((range, theme::brace_match()));
                }
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
    pub(super) fn shape_bidi_row(
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

    pub(super) fn shape_row(
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
    pub(super) fn x_with_virtual(
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
}

pub(super) fn shape(window: &mut Window, text: &str, metrics: &Metrics, color: Hsla) -> ShapedLine {
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
