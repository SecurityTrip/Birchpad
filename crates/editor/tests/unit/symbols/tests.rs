use birchpad_commands::Invocation;
use birchpad_config::Settings;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::*;
use crate::test_support::Workspace;
use crate::test_support::{active_text, document_start, open_workspace, secondary};
use birchpad_ui::AppState;

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
        let column_x = |column: f32| (layout.column_zero + layout.metrics.cell * column).round();
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
