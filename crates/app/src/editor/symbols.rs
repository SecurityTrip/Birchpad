//! What the editor draws besides the text to show its structure: View > Show Symbol (spaces,
//! tabs, line endings, indentation guides, wrap symbols), the vertical edge and the current line,
//! as Notepad++ draws them.

use birchpad_config::{CurrentLine, Edge};
use birchpad_core::motion::line_of;
use birchpad_core::{LineEnding, Rope};
use gpui_kit::{Bounds, Hsla, Pixels, ShapedLine, TextRun, Window, point, px, size};

use super::layout::{Layout, VisibleRow};
use super::{EditorView, ViewSettings, theme};

/// Positions of the visual aids in one frame.
#[derive(Default)]
pub(super) struct Symbols {
    /// The rows of the primary caret's line, across the text area.
    pub(super) current_line: Option<Bounds<Pixels>>,
    /// Draw the current line as a frame of this width rather than a background.
    pub(super) current_line_frame: Option<Pixels>,
    /// Vertical lines at the edge columns, across the text area.
    pub(super) edge_lines: Vec<Bounds<Pixels>>,
    /// Text past the edge column, in background mode.
    pub(super) edge_backgrounds: Vec<Bounds<Pixels>>,
    /// Indentation guides, one pixel wide, drawn dotted.
    pub(super) indent_guides: Vec<Bounds<Pixels>>,
    /// A dot for each space.
    pub(super) spaces: Vec<Bounds<Pixels>>,
    /// The cells of each tab, drawn as an arrow across them.
    pub(super) tabs: Vec<Bounds<Pixels>>,
    /// Boxes labeled CR, LF or CRLF after the end of lines.
    pub(super) line_breaks: Vec<(Bounds<Pixels>, ShapedLine)>,
    /// The cells where wrapped rows show the wrap symbol.
    pub(super) wrap_marks: Vec<Bounds<Pixels>>,
}

impl EditorView {
    /// Computes the visual aids for the rows of `layout`.
    pub(super) fn layout_symbols(
        &mut self,
        layout: &mut Layout,
        text: &Rope,
        settings: &ViewSettings,
        window: &mut Window,
    ) {
        let mut symbols = Symbols::default();
        if settings.current_line != CurrentLine::Off {
            symbols.current_line = self.current_line_bounds(layout, text);
            symbols.current_line_frame = (settings.current_line == CurrentLine::Frame)
                .then_some(settings.current_line_frame_width);
        }
        let cell = layout.metrics.cell;
        match settings.edge {
            Edge::Off => {}
            Edge::Line => {
                let area = layout.text_bounds;
                symbols.edge_lines = settings
                    .edge_columns
                    .iter()
                    .map(|&column| {
                        let x = (layout.column_zero + cell * column as f32).round();
                        Bounds::new(point(x, area.top()), size(px(1.), area.size.height))
                    })
                    .collect();
            }
            Edge::Background => {
                if let Some(&column) = settings.edge_columns.first() {
                    symbols.edge_backgrounds = self.edge_backgrounds(layout, text, column);
                }
            }
        }
        if settings.indent_guides {
            symbols.indent_guides = self.indent_guides(layout, text, settings.tab_width);
        }
        if settings.show_whitespace {
            whitespace_marks(layout, text, &mut symbols);
        }
        if settings.show_eol {
            symbols.line_breaks = self.line_break_marks(layout, text, window);
        }
        if settings.wrap_symbol
            && let Some(wrap_width) = self.display.config().wrap_width
        {
            let x = layout.column_zero + cell * wrap_width as f32;
            symbols.wrap_marks = layout
                .rows
                .iter()
                .filter(|row| !row.row.last_in_line)
                .map(|row| Bounds::new(point(x, row.y), size(cell, layout.metrics.line_height)))
                .collect();
        }
        layout.symbols = symbols;
    }

    fn current_line_bounds(&self, layout: &Layout, text: &Rope) -> Option<Bounds<Pixels>> {
        let line = line_of(text, self.selection.primary().head);
        let mut rows = layout.rows.iter().filter(|row| row.row.line == line);
        let first = rows.next()?;
        let last = rows.next_back().unwrap_or(first);
        let area = layout.text_bounds;
        Some(Bounds::from_corners(
            point(area.left(), first.y),
            point(area.right(), last.y + layout.metrics.line_height),
        ))
    }

    /// The text at or past `column` of its line in each visible row.
    fn edge_backgrounds(
        &mut self,
        layout: &Layout,
        text: &Rope,
        column: usize,
    ) -> Vec<Bounds<Pixels>> {
        let mut quads = Vec::new();
        for row in &layout.rows {
            let start = if row.row.start_column >= column {
                row.row.range.start
            } else {
                self.display.pos_at_column(text, &row.row, column)
            };
            if start >= row.row.range.end {
                continue;
            }
            let left = self.x_in_row(layout, row, start, text);
            let right = self.x_in_row(layout, row, row.row.range.end, text);
            quads.push(Bounds::from_corners(
                point(left, row.y),
                point(right, row.y + layout.metrics.line_height),
            ));
        }
        quads
    }

    /// Guides of the first row of each visible line, joined into runs down the rows.
    fn indent_guides(&self, layout: &Layout, text: &Rope, tab_width: usize) -> Vec<Bounds<Pixels>> {
        let folds = self
            .fold_cache
            .as_ref()
            .map_or(&[][..], |cache| cache.folds.as_slice());
        let is_header = |line: usize| birchpad_view::fold_at(folds, line).is_some();
        let line_height = layout.metrics.line_height;
        let mut guides: Vec<Bounds<Pixels>> = Vec::new();
        for row in layout.rows.iter().filter(|row| row.row.index_in_line == 0) {
            for column in birchpad_view::indent_guides(text, row.row.line, tab_width, is_header) {
                let x = (layout.column_zero + layout.metrics.cell * column as f32).round();
                guides.push(Bounds::new(point(x, row.y), size(px(1.), line_height)));
            }
        }
        guides.sort_by(|a, b| {
            (a.left(), a.top())
                .partial_cmp(&(b.left(), b.top()))
                .unwrap()
        });
        let mut runs: Vec<Bounds<Pixels>> = Vec::new();
        for guide in guides {
            match runs.last_mut() {
                Some(run) if run.left() == guide.left() && run.bottom() == guide.top() => {
                    run.size.height += guide.size.height;
                }
                _ => runs.push(guide),
            }
        }
        runs
    }

    fn line_break_marks(
        &mut self,
        layout: &Layout,
        text: &Rope,
        window: &mut Window,
    ) -> Vec<(Bounds<Pixels>, ShapedLine)> {
        let metrics = &layout.metrics;
        let font_size = (metrics.font_size * 0.7).round();
        let height = (font_size * 1.3).round().min(metrics.line_height);
        let padding = (font_size * 0.25).round();
        let color: Hsla = theme::paint(theme::editor().eol_text).into();
        let mut labels: [Option<ShapedLine>; 3] = Default::default();
        let mut marks = Vec::new();
        for row in layout.rows.iter().filter(|row| row.row.last_in_line) {
            let Some(ending) = LineEnding::at(text, row.row.range.end) else {
                continue;
            };
            let label = labels[ending as usize]
                .get_or_insert_with(|| {
                    let label = ending.label();
                    let run = TextRun {
                        len: label.len(),
                        font: metrics.font.clone(),
                        color,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    window
                        .text_system()
                        .shape_line(label.into(), font_size, &[run], None)
                })
                .clone();
            let x = self.x_in_row(layout, row, row.row.range.end, text) + padding;
            let y = row.y + (metrics.line_height - height) / 2.;
            let bounds = Bounds::new(point(x, y), size(label.width() + padding * 2., height));
            marks.push((bounds, label));
        }
        marks
    }
}

/// Dots for spaces and arrows for tabs in the shaped part of each visible row.
fn whitespace_marks(layout: &Layout, text: &Rope, symbols: &mut Symbols) {
    let metrics = &layout.metrics;
    let dot = (metrics.font_size / 7.).round().max(px(1.));
    for row in &layout.rows {
        let mut glyphs = GlyphCursor::new(row);
        for (_, ch, display) in row.display.chars(text) {
            if ch != ' ' && ch != '\t' {
                continue;
            }
            let left = glyphs.x_at(display.start);
            let right = glyphs.x_at(display.end);
            if ch == ' ' {
                let center = point(
                    ((left + right - dot) / 2.).round(),
                    (row.y + (metrics.line_height - dot) / 2.).round(),
                );
                symbols.spaces.push(Bounds::new(center, size(dot, dot)));
            } else {
                symbols.tabs.push(Bounds::from_corners(
                    point(left, row.y),
                    point(right, row.y + metrics.line_height),
                ));
            }
        }
    }
}

/// x of display offsets of a shaped row, asked for in increasing order: one pass over the
/// glyphs where `ShapedLine::x_for_index` scans from the start each time.
struct GlyphCursor<'a> {
    row: &'a VisibleRow,
    glyphs: Vec<(usize, Pixels)>,
    next: usize,
}

impl<'a> GlyphCursor<'a> {
    fn new(row: &'a VisibleRow) -> Self {
        let glyphs = row
            .shaped
            .runs
            .iter()
            .flat_map(|run| run.glyphs.iter())
            .map(|glyph| (glyph.index, glyph.position.x))
            .collect();
        Self {
            row,
            glyphs,
            next: 0,
        }
    }

    fn x_at(&mut self, offset: usize) -> Pixels {
        while self
            .glyphs
            .get(self.next)
            .is_some_and(|&(index, _)| index < offset)
        {
            self.next += 1;
        }
        let x = self
            .glyphs
            .get(self.next)
            .map_or(self.row.shaped.width(), |&(_, x)| x);
        self.row.x + x
    }
}

#[cfg(test)]
mod tests {
    use birchpad_commands::Invocation;
    use birchpad_config::Settings;
    use gpui_kit::{Entity, TestAppContext, VisualTestContext};

    use super::*;
    use crate::app_state::AppState;
    use crate::workspace::Workspace;
    use crate::workspace::tests::{active_text, document_start, open_workspace, secondary};

    fn view(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<EditorView> {
        workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap())
    }

    fn run(workspace: &Entity<Workspace>, command: &str, cx: &mut VisualTestContext) {
        workspace.update_in(cx, |workspace, window, cx| {
            workspace
                .dispatch(&Invocation::new(command), window, cx)
                .unwrap();
        });
        cx.run_until_parked();
    }

    fn settings(cx: &mut VisualTestContext, change: impl FnOnce(&mut Settings)) {
        cx.update(|window, cx| {
            change(&mut cx.global_mut::<AppState>().settings);
            window.refresh();
        });
        cx.run_until_parked();
    }

    fn view_settings(cx: &mut VisualTestContext) -> ViewSettings {
        cx.update(|_, cx| ViewSettings::read(cx))
    }

    /// Reads the last frame's layout.
    fn with_layout<R>(
        workspace: &Entity<Workspace>,
        cx: &mut VisualTestContext,
        read: impl FnOnce(&Layout) -> R,
    ) -> R {
        let view = view(workspace, cx);
        view.read_with(cx, |view, _| {
            read(view.layout.as_ref().expect("a frame was drawn"))
        })
    }

    fn set_text(text: &str, cx: &mut VisualTestContext) {
        cx.simulate_input(text);
        cx.simulate_keystrokes(document_start());
    }

    #[gpui_kit::test]
    fn show_symbol_switches_toggle_and_are_remembered(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        let shown = |cx: &mut VisualTestContext| {
            let view = view_settings(cx);
            (view.show_whitespace, view.show_eol, view.indent_guides)
        };
        assert_eq!(shown(cx), (false, false, true), "the defaults of Notepad++");

        run(&workspace, "view.show-whitespace", cx);
        assert_eq!(shown(cx), (true, false, true));
        let state = cx.update(|_, cx| AppState::global(cx).state.clone());
        assert_eq!(state.show_whitespace, Some(true));
        assert_eq!(state.show_eol, None);

        // Show All Characters turns on both, then off both.
        run(&workspace, "view.show-all-characters", cx);
        assert_eq!(shown(cx), (true, true, true));
        run(&workspace, "view.show-all-characters", cx);
        assert_eq!(shown(cx), (false, false, true));

        run(&workspace, "view.indent-guides", cx);
        assert_eq!(shown(cx), (false, false, false));
        let state = cx.update(|_, cx| AppState::global(cx).state.clone());
        assert_eq!(state.indent_guides, Some(false));
    }

    #[gpui_kit::test]
    fn spaces_tabs_and_line_breaks_are_marked(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        set_text("a b\tc\r\nd\re\nf", cx);
        assert_eq!(active_text(&workspace, cx), "a b\tc\r\nd\re\nf");
        assert!(with_layout(&workspace, cx, |layout| {
            layout.symbols.spaces.is_empty() && layout.symbols.line_breaks.is_empty()
        }));

        run(&workspace, "view.show-all-characters", cx);
        with_layout(&workspace, cx, |layout| {
            let symbols = &layout.symbols;
            let row = &layout.rows[0];
            let x = |pos| row.x_for(pos).unwrap();
            assert_eq!(symbols.spaces.len(), 1);
            let dot = symbols.spaces[0];
            assert!(
                x(1) <= dot.left() && dot.right() <= x(2),
                "inside the space"
            );
            assert_eq!(symbols.tabs.len(), 1);
            assert_eq!(
                (symbols.tabs[0].left(), symbols.tabs[0].right()),
                (x(3), x(4))
            );
            let labels: Vec<&str> = symbols
                .line_breaks
                .iter()
                .map(|(_, label)| label.text.as_ref())
                .collect();
            assert_eq!(labels, ["CRLF", "CR", "LF"], "none after the last line");
            assert!(symbols.line_breaks[0].0.left() > x(5), "after the text");
        });
    }

    #[gpui_kit::test]
    fn indent_guides_run_through_indentation_and_blank_lines(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        set_text("a {\n        b\n\n    c\n}", cx);
        with_layout(&workspace, cx, |layout| {
            let x = (layout.column_zero + layout.metrics.cell * 4.).round();
            let line_height = layout.metrics.line_height;
            // Line 1 (two levels) and the blank line after it in one run; line 3 has one
            // level, so no guide.
            assert_eq!(
                layout.symbols.indent_guides,
                [Bounds::new(
                    point(x, layout.rows[1].y),
                    size(px(1.), line_height * 2.)
                )]
            );
        });
        run(&workspace, "view.indent-guides", cx);
        assert!(with_layout(&workspace, cx, |layout| {
            layout.symbols.indent_guides.is_empty()
        }));
    }

    #[gpui_kit::test]
    fn edge_as_lines_or_background(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        set_text("abcdefgh\nab", cx);
        assert!(with_layout(&workspace, cx, |layout| {
            layout.symbols.edge_lines.is_empty() && layout.symbols.edge_backgrounds.is_empty()
        }));

        settings(cx, |settings| {
            settings.editor.edge = Edge::Line;
            settings.editor.edge_columns = vec![4, 8];
        });
        with_layout(&workspace, cx, |layout| {
            let column_x =
                |column: f32| (layout.column_zero + layout.metrics.cell * column).round();
            let lines: Vec<Pixels> = layout
                .symbols
                .edge_lines
                .iter()
                .map(|line| line.left())
                .collect();
            assert_eq!(lines, [column_x(4.), column_x(8.)]);
        });

        settings(cx, |settings| settings.editor.edge = Edge::Background);
        with_layout(&workspace, cx, |layout| {
            let row = &layout.rows[0];
            let backgrounds = &layout.symbols.edge_backgrounds;
            assert!(layout.symbols.edge_lines.is_empty());
            assert_eq!(backgrounds.len(), 1, "the second line is shorter");
            assert_eq!(
                (backgrounds[0].left(), backgrounds[0].right()),
                (row.x_for(4).unwrap(), row.x_for(8).unwrap())
            );
        });
    }

    #[gpui_kit::test]
    fn current_line_follows_the_primary_caret(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        set_text("one\ntwo\nthree", cx);
        cx.simulate_keystrokes("down");
        with_layout(&workspace, cx, |layout| {
            let line = layout.symbols.current_line.expect("highlighted by default");
            assert_eq!(line.top(), layout.rows[1].y);
            assert_eq!(line.size.height, layout.metrics.line_height);
            assert_eq!(line.size.width, layout.text_bounds.size.width);
            assert_eq!(layout.symbols.current_line_frame, None);
        });

        settings(cx, |settings| {
            settings.editor.current_line = CurrentLine::Frame;
            settings.editor.current_line_frame_width = 3;
        });
        assert_eq!(
            with_layout(&workspace, cx, |layout| layout.symbols.current_line_frame),
            Some(px(3.))
        );
        settings(cx, |settings| {
            settings.editor.current_line = CurrentLine::Off;
        });
        assert!(with_layout(&workspace, cx, |layout| {
            layout.symbols.current_line.is_none()
        }));
    }

    #[gpui_kit::test]
    fn wrapped_rows_end_with_the_wrap_symbol(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        set_text(&"word ".repeat(200), cx);
        run(&workspace, "view.word-wrap", cx);
        let columns = with_layout(&workspace, cx, |layout| layout.text_columns());
        let wrap_width = |cx: &mut VisualTestContext| {
            view(&workspace, cx).read_with(cx, |view, _| view.display.config().wrap_width)
        };
        assert_eq!(wrap_width(cx), Some(columns));

        // The symbol takes the last column.
        run(&workspace, "view.wrap-symbol", cx);
        assert_eq!(wrap_width(cx), Some(columns - 1));
        with_layout(&workspace, cx, |layout| {
            let rows = &layout.rows;
            assert!(rows.len() > 2);
            let marks = &layout.symbols.wrap_marks;
            assert_eq!(marks.len(), rows.len() - 1, "all rows but the last");
            let x = layout.column_zero + layout.metrics.cell * (columns - 1) as f32;
            assert!(marks.iter().all(|mark| mark.left() == x));
        });
    }

    #[gpui_kit::test]
    fn split_lines_at_the_edge_or_the_view_width(cx: &mut TestAppContext) {
        let (workspace, cx) = open_workspace(cx);
        let eol = birchpad_core::LineEnding::native().as_str();
        set_text("aaa bbb ccc ddd", cx);
        settings(cx, |settings| {
            settings.editor.edge = Edge::Line;
            settings.editor.edge_columns = vec![8, 40];
        });
        run(&workspace, "edit.split-lines", cx);
        assert_eq!(
            active_text(&workspace, cx),
            format!("aaa bbb{eol}ccc ddd"),
            "at the first edge column"
        );

        // Without the edge, at the width of the view.
        settings(cx, |settings| settings.editor.edge = Edge::Off);
        let columns = with_layout(&workspace, cx, |layout| layout.text_columns());
        cx.simulate_keystrokes(&secondary("a"));
        set_text("abcdefghi ".repeat(columns / 4).trim_end(), cx);
        run(&workspace, "edit.split-lines", cx);
        let text = active_text(&workspace, cx);
        let lines: Vec<&str> = text.split(eol).collect();
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|line| line.len() <= columns));
        assert!(lines[0].len() > columns - 10, "filled up to the width");
    }
}
