//! Preferences > Editing and Margins left from Notepad++ 8 (roadmap 7.4b).

use birchpad_commands::Invocation;
use birchpad_config::{
    CaretWidth, EditorSettings, EolStyle, FoldStyle, NonPrintingStyle, WrapIndent,
};
use birchpad_core::{Range, Selection};
use birchpad_editor::*;
use birchpad_ui::AppState;
use birchpad_view::{Block, BlockPoint};
use gpui_kit::{
    Entity, Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext, point, px,
};

use crate::test_support::{Workspace, active_text, open_workspace};

fn view(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<EditorView> {
    workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap())
}

fn set(cx: &mut VisualTestContext, change: impl FnOnce(&mut EditorSettings)) {
    cx.update(|_, cx| change(&mut cx.global_mut::<AppState>().settings.editor));
}

/// Redraws `view` after a setting changed.
fn redraw(view: &Entity<EditorView>, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| view.settings_changed(cx));
    cx.run_until_parked();
}

fn run(workspace: &Entity<Workspace>, id: &str, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace
            .dispatch(&Invocation::new(id), window, cx)
            .unwrap();
    });
    cx.run_until_parked();
}

fn select(view: &Entity<EditorView>, from: usize, to: usize, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| view.select_range(from..to, cx));
    cx.run_until_parked();
}

fn selection(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> Selection {
    view.read_with(cx, |view, _| view.selection.clone())
}

/// The window point at `column` (cells from the text's start) of visible row `row`.
fn at(
    view: &Entity<EditorView>,
    row: usize,
    column: f32,
    cx: &mut VisualTestContext,
) -> Point<Pixels> {
    view.read_with(cx, |view, _| {
        let layout = view.layout.as_ref().expect("drawn");
        let visible = &layout.rows[row];
        point(
            layout.column_zero + layout.metrics.cell * column,
            visible.y + layout.metrics.line_height / 2.,
        )
    })
}

#[test]
fn control_characters_are_left_out_of_what_keys_type() {
    assert_eq!(without_control_characters("a\u{5}b"), "ab");
    assert_eq!(without_control_characters("\t\r\n"), "\t\r\n");
    assert_eq!(without_control_characters("\u{1b}"), "");
    assert_eq!(without_control_characters("мир"), "мир");
    assert_eq!(without_control_characters(""), "");
}

#[gpui_kit::test]
fn typing_a_control_character_types_nothing_unless_allowed(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("a\u{5}b");
    assert_eq!(active_text(&workspace, cx), "ab");
    cx.simulate_input("\u{5}");
    assert_eq!(active_text(&workspace, cx), "ab");
    set(cx, |editor| editor.prevent_control_typing = false);
    cx.simulate_input("\u{5}");
    assert_eq!(active_text(&workspace, cx), "ab\u{5}");
}

#[gpui_kit::test]
fn the_caret_takes_the_width_asked_for(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("ab");
    let view = view(&workspace, cx);
    let carets = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            let layout = view.layout.as_ref().unwrap();
            (
                layout
                    .carets
                    .iter()
                    .map(|caret| caret.size.width)
                    .collect::<Vec<_>>(),
                layout.metrics.cell,
                layout.caret_block,
            )
        })
    };
    assert_eq!(carets(cx).0, [px(2.)], "two pixels by default");
    for (width, pixels) in [(CaretWidth::One, 1.), (CaretWidth::Three, 3.)] {
        set(cx, |editor| editor.caret_width = width);
        redraw(&view, cx);
        assert_eq!(carets(cx).0, [px(pixels)]);
    }
    set(cx, |editor| editor.caret_width = CaretWidth::Block);
    redraw(&view, cx);
    let (widths, cell, block) = carets(cx);
    assert_eq!(widths, [cell]);
    assert!(block);
    set(cx, |editor| editor.caret_width = CaretWidth::None);
    redraw(&view, cx);
    assert!(carets(cx).0.is_empty());
}

#[gpui_kit::test]
fn scrolling_beyond_the_last_line_goes_up_to_it(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input(&"line\n".repeat(200));
    let view = view(&workspace, cx);
    let top_after = |cx: &mut VisualTestContext| {
        view.update(cx, |view, cx| view.scroll_to_line(200, cx));
        cx.run_until_parked();
        view.read_with(cx, |view, _| view.scroll_top)
    };
    let usual = top_after(cx);
    assert!(usual < 200.);
    set(cx, |editor| editor.scroll_beyond_last_line = true);
    redraw(&view, cx);
    assert_eq!(top_after(cx), 200.);
}

#[gpui_kit::test]
fn copy_and_cut_take_the_line_when_asked_to(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("one\ntwo\nthree");
    let view = view(&workspace, cx);
    select(&view, 5, 5, cx);
    cx.update(|_, cx| cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("old".into())));
    run(&workspace, "edit.copy", cx);
    let clipboard = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
    };
    assert_eq!(clipboard(cx).as_deref(), Some("old"), "off by default");
    set(cx, |editor| editor.copy_line_without_selection = true);
    run(&workspace, "edit.copy", cx);
    assert_eq!(clipboard(cx).as_deref(), Some("two\n"));
    run(&workspace, "edit.cut", cx);
    assert_eq!(active_text(&workspace, cx), "one\nthree");
    // The last line has no line break to take.
    select(&view, 6, 6, cx);
    run(&workspace, "edit.cut", cx);
    assert_eq!(active_text(&workspace, cx), "one\n");
    assert_eq!(clipboard(cx).as_deref(), Some("three"));
}

#[gpui_kit::test]
fn dragging_the_selection_moves_or_copies_it(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("hello world");
    let view = view(&workspace, cx);
    select(&view, 0, 5, cx);
    view.update(cx, |view, cx| view.drop_selection(11, false, cx));
    assert_eq!(active_text(&workspace, cx), " worldhello");
    assert_eq!(selection(&view, cx).primary(), Range::new(6, 11));
    view.update(cx, |view, cx| view.drop_selection(0, true, cx));
    assert_eq!(active_text(&workspace, cx), "hello worldhello");
    // Dropped inside itself: nothing happens.
    select(&view, 0, 5, cx);
    view.update(cx, |view, cx| view.drop_selection(3, false, cx));
    assert_eq!(active_text(&workspace, cx), "hello worldhello");
    // Undo puts the text back in one step.
    run(&workspace, "edit.undo", cx);
    assert_eq!(active_text(&workspace, cx), " worldhello");
}

#[gpui_kit::test]
fn a_drag_in_the_selection_drops_it_with_the_mouse(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("abc def");
    let view = view(&workspace, cx);
    select(&view, 0, 3, cx);
    let from = at(&view, 0, 1.5, cx);
    let to = at(&view, 0, 7.2, cx);
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    assert_eq!(active_text(&workspace, cx), " defabc");
    // Turned off, the same press selects from there instead.
    set(cx, |editor| editor.drag_text = false);
    select(&view, 0, 3, cx);
    let point_1 = at(&view, 0, 1.5, cx);
    cx.simulate_mouse_down(point_1, MouseButton::Left, Modifiers::none());
    let point_2 = at(&view, 0, 1.5, cx);
    cx.simulate_mouse_up(point_2, MouseButton::Left, Modifiers::none());
    assert_eq!(active_text(&workspace, cx), " defabc");
    assert!(selection(&view, cx).primary().is_empty());
}

#[gpui_kit::test]
fn ctrl_click_adds_carets_only_with_multi_editing(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("abcdef");
    let view = view(&workspace, cx);
    select(&view, 1, 1, cx);
    let point_3 = at(&view, 0, 4., cx);
    cx.simulate_click(point_3, Modifiers::secondary_key());
    assert_eq!(selection(&view, cx).ranges().len(), 2);
    set(cx, |editor| editor.multi_editing = false);
    select(&view, 1, 1, cx);
    let point_4 = at(&view, 0, 4., cx);
    cx.simulate_click(point_4, Modifiers::secondary_key());
    assert_eq!(selection(&view, cx).ranges().len(), 1);
}

#[gpui_kit::test]
fn a_right_click_keeps_the_selection_when_asked_to(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("abc def");
    let view = view(&workspace, cx);
    let right_click = |cx: &mut VisualTestContext| {
        let outside = at(&view, 0, 6., cx);
        cx.simulate_mouse_down(outside, MouseButton::Right, Modifiers::none());
        cx.simulate_mouse_up(outside, MouseButton::Right, Modifiers::none());
        cx.run_until_parked();
    };
    select(&view, 0, 3, cx);
    right_click(cx);
    assert!(
        selection(&view, cx).primary().is_empty(),
        "the caret moves by default"
    );
    set(cx, |editor| editor.keep_selection_on_right_click = true);
    select(&view, 0, 3, cx);
    right_click(cx);
    assert_eq!(selection(&view, cx).primary(), Range::new(0, 3));
}

#[gpui_kit::test]
fn a_click_past_the_line_s_end_stays_there_in_virtual_space(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("ab");
    let view = view(&workspace, cx);
    let point_5 = at(&view, 0, 6., cx);
    cx.simulate_click(point_5, Modifiers::none());
    assert_eq!(
        selection(&view, cx).primary().head_virtual,
        0,
        "off by default"
    );
    set(cx, |editor| editor.virtual_space = true);
    let point_6 = at(&view, 0, 6., cx);
    cx.simulate_click(point_6, Modifiers::none());
    let caret = selection(&view, cx).primary();
    assert_eq!((caret.head, caret.head_virtual), (2, 4));
    // Typing there fills the gap with spaces.
    cx.simulate_input("x");
    assert_eq!(active_text(&workspace, cx), "ab    x");
}

#[gpui_kit::test]
fn a_rectangle_typed_in_becomes_multi_editing(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("aaa\nbbb");
    let view = view(&workspace, cx);
    set(cx, |editor| editor.column_to_multi_editing = true);
    view.update(cx, |view, cx| {
        view.set_block(Block::new(BlockPoint::new(0, 1), BlockPoint::new(1, 1)), cx)
    });
    assert!(view.read_with(cx, |view, _| view.active_block().is_some()));
    cx.simulate_input("x");
    assert_eq!(active_text(&workspace, cx), "axaa\nbxbb");
    assert!(view.read_with(cx, |view, _| view.active_block().is_none()));
    assert_eq!(selection(&view, cx).ranges().len(), 2);
}

#[gpui_kit::test]
fn margins_take_their_style_border_and_padding(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("a");
    let view = view(&workspace, cx);
    let left = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.layout.as_ref().unwrap().text_bounds.left()
        })
    };
    let before = left(cx);
    set(cx, |editor| {
        editor.padding_left = 10;
        editor.fold_style = FoldStyle::None;
        editor.border_width = 3;
    });
    redraw(&view, cx);
    assert_eq!(left(cx), before + px(10.));
    view.read_with(cx, |view, _| {
        let layout = view.layout.as_ref().unwrap();
        assert_eq!(layout.fold_style, FoldStyle::None);
        assert_eq!(layout.border_width, px(3.));
    });
    // A constant line number margin is as wide for one line as for a million.
    let numbers = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.layout
                .as_ref()
                .unwrap()
                .margins
                .line_numbers
                .unwrap()
                .size
                .width
        })
    };
    let dynamic = numbers(cx);
    set(cx, |editor| {
        editor.line_number_width = birchpad_config::LineNumberWidth::Constant
    });
    redraw(&view, cx);
    assert!(numbers(cx) > dynamic);
}

#[gpui_kit::test]
fn line_endings_show_as_plain_text_when_asked_to(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("a\nb");
    run(&workspace, "view.show-eol", cx);
    let view = view(&workspace, cx);
    let plain = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            let symbols = &view.layout.as_ref().unwrap().symbols;
            (symbols.eol_plain, symbols.line_breaks.len())
        })
    };
    assert_eq!(plain(cx), (false, 1));
    set(cx, |editor| editor.eol_style = EolStyle::PlainText);
    redraw(&view, cx);
    assert_eq!(plain(cx), (true, 1));
}

#[gpui_kit::test]
fn wrapped_rows_start_under_the_indentation(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let long = format!("        {}", "word ".repeat(200));
    cx.simulate_input(&long);
    run(&workspace, "view.word-wrap", cx);
    let view = view(&workspace, cx);
    let xs = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            let rows = &view.layout.as_ref().unwrap().rows;
            (rows[0].x, rows[1].x)
        })
    };
    let (first, second) = xs(cx);
    assert_eq!(first, second, "the left edge by default");
    set(cx, |editor| editor.wrap_indent = WrapIndent::Aligned);
    redraw(&view, cx);
    let (first, second) = xs(cx);
    let cell = view.read_with(cx, |view, _| view.layout.as_ref().unwrap().metrics.cell);
    assert_eq!(second, first + cell * 8.);
}

#[gpui_kit::test]
fn non_printing_characters_show_by_label_when_asked_to(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("a\u{A0}b\u{200B}c");
    let view = view(&workspace, cx);
    let shown = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            let layout = view.layout.as_ref().unwrap();
            (
                layout.rows[0].display.text.clone(),
                layout.symbols.odd_spaces.len(),
                layout.symbols.zero_widths.len(),
            )
        })
    };
    let end_column = |cx: &mut VisualTestContext| {
        view.update(cx, |view, cx| {
            let text = view.buffer.read(cx).doc().text().clone();
            view.display.column(&text, text.len())
        })
    };
    // A label alone does nothing while the characters are not shown.
    set(cx, |editor| {
        editor.non_printing_style = NonPrintingStyle::Abbreviation
    });
    redraw(&view, cx);
    assert_eq!(shown(cx), ("a\u{A0}b\u{200B}c".to_owned(), 0, 0));
    assert_eq!(end_column(cx), 4);
    // Shown: framed and marked by default, in their own cells.
    set(cx, |editor| {
        editor.non_printing_style = NonPrintingStyle::Marks
    });
    run(&workspace, "view.show-non-printing", cx);
    assert_eq!(shown(cx), ("a\u{A0}b\u{200B}c".to_owned(), 1, 1));
    // By abbreviation, each in a frame of its own, the label's cells taken.
    set(cx, |editor| {
        editor.non_printing_style = NonPrintingStyle::Abbreviation
    });
    redraw(&view, cx);
    assert_eq!(shown(cx), ("aNBSPbZWSPc".to_owned(), 2, 0));
    assert_eq!(end_column(cx), 11);
    // By code point.
    set(cx, |editor| {
        editor.non_printing_style = NonPrintingStyle::CodePoint
    });
    redraw(&view, cx);
    assert_eq!(shown(cx).0, "aU+00A0bU+200Bc");
    assert_eq!(end_column(cx), 15);
    // The caret steps over a label as over one character.
    view.update(cx, |view, cx| view.select_range(1..1, cx));
    cx.simulate_keystrokes("right");
    assert_eq!(selection(&view, cx).primary().head, 3);
    // The text is unchanged.
    assert_eq!(active_text(&workspace, cx), "a\u{A0}b\u{200B}c");
}

#[gpui_kit::test]
fn sql_strings_follow_the_backslash_setting(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    workspace.update_in(cx, |workspace, window, cx| {
        workspace
            .dispatch(
                &Invocation::with_args("language.set", serde_json::json!({ "language": "sql" })),
                window,
                cx,
            )
            .unwrap();
    });
    cx.run_until_parked();
    let source = r"SELECT 'it\'s', 2;";
    workspace.update_in(cx, |workspace, window, cx| {
        workspace
            .dispatch(
                &Invocation::with_args("edit.insert-text", serde_json::json!({ "text": source })),
                window,
                cx,
            )
            .unwrap();
    });
    cx.run_until_parked();
    let view = view(&workspace, cx);
    let whole_string = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, cx| {
            let buffer = view.buffer.read(cx);
            let text = buffer.doc().text().clone();
            let syntax = buffer.syntax().expect("SQL has a grammar");
            syntax
                .highlights(&text, 0..text.len())
                .iter()
                .any(|(range, highlight)| {
                    highlight.name() == "string" && text.slice(range.clone()) == r"'it\'s'"
                })
        })
    };
    assert!(!whole_string(cx), "off by default, as standard SQL");
    cx.update(|_, cx| cx.global_mut::<AppState>().settings.sql.backslash_escapes = true);
    redraw(&view, cx);
    assert!(whole_string(cx));
    // Off again, the text is read as before.
    cx.update(|_, cx| cx.global_mut::<AppState>().settings.sql.backslash_escapes = false);
    redraw(&view, cx);
    assert!(!whole_string(cx));
    assert_eq!(active_text(&workspace, cx), source);
}
