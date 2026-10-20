use birchpad_commands::Invocation;
use birchpad_core::LineEnding;
use gpui_kit::{
    AppContext as _, Entity, Modifiers, MouseButton, Pixels, Point, TestAppContext,
    VisualTestContext, point,
};
use serde_json::json;

use crate::test_support::Workspace;
use crate::test_support::{
    active_text, begin_end_column, block, column_editor, document_start, open_workspace, secondary,
};
use birchpad_core::Range;
use birchpad_editor::*;
use birchpad_view::Block;
use birchpad_view::BlockPoint;
use gpui_kit::ClipboardItem;

fn view(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<EditorView> {
    workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap())
}

fn run(workspace: &Entity<Workspace>, invocation: Invocation, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&invocation, window, cx).unwrap();
    });
    cx.run_until_parked();
}

fn ranges(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<Range> {
    let view = view(workspace, cx);
    view.read_with(cx, |view, _| view.selection.ranges().to_vec())
}

fn primary(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Range {
    let view = view(workspace, cx);
    view.read_with(cx, |view, _| view.selection.primary())
}

fn in_block(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<Block> {
    let view = view(workspace, cx);
    view.read_with(cx, |view, _| view.active_block())
}

/// The window point at `column` cells of `line` (an unwrapped, visible line).
fn cell_point(
    workspace: &Entity<Workspace>,
    line: usize,
    column: usize,
    cx: &mut VisualTestContext,
) -> Point<Pixels> {
    let view = view(workspace, cx);
    view.read_with(cx, |view, _| {
        let layout = view.layout.as_ref().expect("a frame was drawn");
        let row = layout.rows.iter().find(|row| row.row.line == line).unwrap();
        point(
            layout.column_zero + layout.metrics.cell * column as f32,
            row.y + layout.metrics.line_height / 2.,
        )
    })
}

fn set_text(text: &str, cx: &mut VisualTestContext) {
    cx.simulate_input(text);
    cx.simulate_keystrokes(document_start());
}

#[gpui_kit::test]
fn alt_drag_selects_a_rectangle_into_virtual_space(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    set_text("abcdef\nab\nabcdef", cx);
    let from = cell_point(&workspace, 0, 1, cx);
    let to = cell_point(&workspace, 2, 4, cx);
    let middle = cell_point(&workspace, 1, 4, cx);
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::alt());
    cx.simulate_mouse_move(middle, MouseButton::Left, Modifiers::alt());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::alt());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::alt());
    assert_eq!(
        ranges(&workspace, cx),
        [
            Range::new(1, 4),
            Range::new(8, 9).with_virtual(0, 2),
            Range::new(11, 14),
        ]
    );
    assert_eq!(
        primary(&workspace, cx),
        Range::new(11, 14),
        "the caret's line"
    );

    // Typing replaces the rectangle on every line and leaves a caret on each.
    cx.simulate_input("X");
    assert_eq!(active_text(&workspace, cx), "aXef\naX\naXef");
    cx.simulate_input("Y");
    assert_eq!(active_text(&workspace, cx), "aXYef\naXY\naXYef");
    assert!(in_block(&workspace, cx).is_some(), "still in column mode");
    cx.simulate_keystrokes(&secondary("z"));
    assert_eq!(
        active_text(&workspace, cx),
        "abcdef\nab\nabcdef",
        "typing in a rectangle is one undo step"
    );
}

#[gpui_kit::test]
fn keys_extend_a_rectangle_and_typing_fills_virtual_space(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    set_text("abcd\n\nab", cx);
    cx.simulate_keystrokes(&format!(
        "right right right {} {}",
        block("down"),
        block("down")
    ));
    assert_eq!(
        ranges(&workspace, cx),
        [
            Range::point(3),
            Range::virtual_point(5, 3),
            Range::virtual_point(8, 1),
        ]
    );
    cx.simulate_input("|");
    assert_eq!(active_text(&workspace, cx), "abc|d\n   |\nab |");
    // Backspace deletes the typed bar on every line.
    cx.simulate_keystrokes("backspace");
    assert_eq!(active_text(&workspace, cx), "abcd\n   \nab ");
    cx.simulate_keystrokes(&secondary("z"));
    cx.simulate_keystrokes(&secondary("z"));
    assert_eq!(active_text(&workspace, cx), "abcd\n\nab");

    // Right goes on into virtual space, Left comes back; Up shrinks the rectangle.
    cx.simulate_keystrokes(document_start());
    cx.simulate_keystrokes(&format!(
        "end {} {} {}",
        block("right"),
        block("right"),
        block("down")
    ));
    assert_eq!(
        ranges(&workspace, cx),
        [
            Range::new(4, 4).with_virtual(0, 2),
            Range::new(5, 5).with_virtual(4, 6),
        ]
    );
    cx.simulate_keystrokes(&format!("{} {}", block("left"), block("up")));
    assert_eq!(
        ranges(&workspace, cx),
        [Range::new(4, 4).with_virtual(0, 1)]
    );

    // Esc leaves column mode with a caret at the caret corner.
    cx.simulate_keystrokes(&format!("{} escape", block("down")));
    assert_eq!(ranges(&workspace, cx), [Range::point(5)]);
    assert!(in_block(&workspace, cx).is_none());
}

#[gpui_kit::test]
fn backspace_in_virtual_space_moves_the_carets(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    set_text("ab\nabcd", cx);
    cx.simulate_keystrokes(&format!("down end {}", block("up")));
    assert_eq!(
        ranges(&workspace, cx),
        [Range::virtual_point(2, 2), Range::point(7)]
    );
    cx.simulate_keystrokes("backspace");
    assert_eq!(active_text(&workspace, cx), "ab\nabc");
    assert_eq!(
        ranges(&workspace, cx),
        [Range::virtual_point(2, 1), Range::point(6)]
    );
    // Delete never joins lines.
    cx.simulate_keystrokes("delete");
    assert_eq!(active_text(&workspace, cx), "ab\nabc");
}

#[gpui_kit::test]
fn copying_and_pasting_a_rectangle(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    set_text("ab12\ncd34\nef", cx);
    cx.simulate_keystrokes(&format!(
        "right right {} {} {}",
        block("down"),
        block("right"),
        block("right")
    ));
    cx.simulate_keystrokes(&secondary("c"));
    let eol = LineEnding::native().as_str();
    let clipboard = cx.read_from_clipboard().unwrap();
    assert_eq!(
        clipboard.text().unwrap(),
        format!("12{eol}34{eol}"),
        "other programs get the rows as text"
    );

    // At the end of "ef": the first row goes there, the second on a new line.
    cx.simulate_keystrokes(&secondary("end"));
    let end = if cfg!(target_os = "macos") {
        "cmd-down"
    } else {
        "ctrl-end"
    };
    cx.simulate_keystrokes(end);
    cx.simulate_keystrokes(&secondary("v"));
    assert_eq!(
        active_text(&workspace, cx),
        format!("ab12\ncd34\nef12{eol}  34")
    );
    cx.simulate_keystrokes(&secondary("z"));
    assert_eq!(active_text(&workspace, cx), "ab12\ncd34\nef");

    // Over a rectangle of the same size: replaces it.
    cx.simulate_keystrokes(document_start());
    cx.simulate_keystrokes(&format!(
        "{} {} {}",
        block("down"),
        block("right"),
        block("right")
    ));
    cx.simulate_keystrokes(&secondary("v"));
    assert_eq!(active_text(&workspace, cx), "1212\n3434\nef");

    // Text copied as a stream pastes into every line of a rectangle.
    cx.write_to_clipboard(ClipboardItem::new_string("-".to_owned()));
    cx.simulate_keystrokes(document_start());
    cx.simulate_keystrokes(&format!("{} {}", block("down"), block("down")));
    cx.simulate_keystrokes(&secondary("v"));
    assert_eq!(active_text(&workspace, cx), "-1212\n-3434\n-ef");
}

#[gpui_kit::test]
fn multi_select_all_renames_every_occurrence(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    set_text("let foo = foo_bar + Foo;\nfoo();", cx);
    cx.simulate_keystrokes("right right right right right");
    let all = Invocation::with_args(
        "edit.multi-select-all",
        json!({ "match-case": true, "whole-word": true }),
    );
    run(&workspace, all, cx);
    assert_eq!(
        ranges(&workspace, cx),
        [Range::new(4, 7), Range::new(25, 28)]
    );
    cx.simulate_input("value");
    assert_eq!(
        active_text(&workspace, cx),
        "let value = foo_bar + Foo;\nvalue();"
    );
    cx.simulate_keystrokes(&secondary("z"));
    assert_eq!(
        active_text(&workspace, cx),
        "let foo = foo_bar + Foo;\nfoo();"
    );
}

#[gpui_kit::test]
fn multi_select_next_undo_latest_and_escape(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    set_text("ab x ab x ab", cx);
    let next = Invocation::new("edit.multi-select-next");
    run(&workspace, next.clone(), cx);
    assert_eq!(ranges(&workspace, cx), [Range::new(0, 2)], "the word first");
    run(&workspace, next.clone(), cx);
    run(&workspace, next, cx);
    assert_eq!(
        ranges(&workspace, cx),
        [Range::new(0, 2), Range::new(5, 7), Range::new(10, 12)]
    );
    assert_eq!(primary(&workspace, cx), Range::new(10, 12));
    run(&workspace, Invocation::new("edit.multi-select-undo"), cx);
    assert_eq!(ranges(&workspace, cx), [Range::new(0, 2), Range::new(5, 7)]);
    assert_eq!(
        primary(&workspace, cx),
        Range::new(5, 7),
        "the one added before"
    );
    run(&workspace, Invocation::new("edit.multi-select-skip"), cx);
    assert_eq!(
        ranges(&workspace, cx),
        [Range::new(0, 2), Range::new(10, 12)]
    );
    cx.simulate_keystrokes("escape");
    assert_eq!(ranges(&workspace, cx), [Range::new(10, 12)]);
}

#[gpui_kit::test]
fn begin_end_select_in_both_modes(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    set_text("abcd\nefgh\nijkl", cx);
    cx.simulate_keystrokes(&format!("right {}", secondary("shift-b")));
    let view = view(&workspace, cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.begin_select_pending()),
        Some(false)
    );
    cx.simulate_keystrokes(&format!("down down {}", secondary("shift-b")));
    assert_eq!(ranges(&workspace, cx), [Range::new(1, 11)]);

    // Right first collapses the selection to its end (line 2, column 1).
    cx.simulate_keystrokes(&format!(
        "right {} right right up {}",
        begin_end_column(),
        begin_end_column()
    ));
    assert_eq!(
        ranges(&workspace, cx),
        [Range::new(6, 8), Range::new(11, 13)]
    );
    assert!(in_block(&workspace, cx).is_some());
    assert_eq!(
        view.read_with(cx, |view, _| view.begin_select_pending()),
        None
    );
}

#[gpui_kit::test]
fn sorting_a_csv_by_a_rectangle(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    set_text("name,age\nbob,30\namy,04\ncat,15", cx);
    // The age column of the three records.
    cx.simulate_keystrokes("down right right right right");
    cx.simulate_keystrokes(&format!(
        "{} {} {} {}",
        block("down"),
        block("down"),
        block("right"),
        block("right")
    ));
    let sort = Invocation::with_args("edit.sort-lines", json!({ "by": "integer" }));
    run(&workspace, sort, cx);
    let eol = LineEnding::native().as_str();
    assert_eq!(
        active_text(&workspace, cx),
        format!("name,age\namy,04{eol}cat,15{eol}bob,30")
    );
    assert!(in_block(&workspace, cx).is_some(), "the rectangle stays");
    cx.simulate_keystrokes(&secondary("z"));
    assert_eq!(
        active_text(&workspace, cx),
        "name,age\nbob,30\namy,04\ncat,15"
    );
}

#[gpui_kit::test]
fn column_editor_numbers_a_thousand_lines_in_one_step(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let original = vec!["x"; 1000].join("\n");
    set_text(&original, cx);
    let view = view(&workspace, cx);
    // The dialog's fields, filled and confirmed.
    cx.update(|window, cx| {
        let editor = cx
            .new(|cx| birchpad_editor::column_editor::ColumnEditor::new(view.clone(), window, cx));
        editor.update(cx, |editor, cx| {
            editor.fill_numbers("1", "1", "", window, cx);
            editor.insert(cx).unwrap();
        });
    });
    cx.run_until_parked();
    let text = active_text(&workspace, cx);
    let lines: Vec<&str> = text.split('\n').collect();
    assert_eq!(lines.len(), 1000);
    assert_eq!(lines[0], "1   x", "left-aligned to the widest number");
    assert_eq!(lines[99], "100 x");
    assert_eq!(lines[999], "1000x");
    cx.simulate_keystrokes(&secondary("z"));
    assert_eq!(active_text(&workspace, cx), original, "one undo step");

    // Alt+C (Cmd+Alt+C on macOS) opens the dialog.
    cx.simulate_keystrokes(column_editor());
    let open = cx.update(|window, cx| {
        use gpui_kit::component::WindowExt as _;
        window.has_active_dialog(cx)
    });
    assert!(open);
}

#[gpui_kit::test]
fn column_insert_into_a_rectangle_and_carets(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    set_text("a\nbbb\nc", cx);
    cx.simulate_keystrokes(&format!("right {} {}", block("down"), block("down")));
    let insert = Invocation::with_args(
        "edit.column-insert",
        json!({ "initial": 9, "leading": "zeros", "format": "dec" }),
    );
    run(&workspace, insert, cx);
    assert_eq!(active_text(&workspace, cx), "a09\nb10bb\nc11");
    let text = Invocation::with_args("edit.column-insert", json!({ "text": "|" }));
    cx.simulate_keystrokes(document_start());
    run(&workspace, text, cx);
    assert_eq!(active_text(&workspace, cx), "|a09\n|b10bb\n|c11");
}

#[gpui_kit::test]
fn input_methods_compose_at_the_primary_caret_only(cx: &mut TestAppContext) {
    use gpui_kit::EntityInputHandler as _;
    let (workspace, cx) = open_workspace(cx);
    set_text("a\nb", cx);
    cx.simulate_keystrokes(&format!("end {}", block("down")));
    let view = view(&workspace, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.replace_and_mark_text_in_range(None, "か", None, window, cx);
            view.replace_text_in_range(None, "漢", window, cx);
        });
    });
    assert_eq!(active_text(&workspace, cx), "a\nb漢");
    assert_eq!(ranges(&workspace, cx).len(), 2, "the other caret stays");
    cx.simulate_input("!");
    assert_eq!(active_text(&workspace, cx), "a!\nb漢!");
}

#[gpui_kit::test]
#[ignore = "a timing; run in release: cargo test --release -- --ignored --nocapture"]
fn typing_with_a_thousand_carets(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    set_text(&vec!["some text on a line"; 1000].join("\n"), cx);
    let view = view(&workspace, cx);
    view.update(cx, |view, cx| {
        view.set_block(
            Block::new(BlockPoint::new(0, 4), BlockPoint::new(999, 4)),
            cx,
        );
    });
    cx.run_until_parked();
    let started = std::time::Instant::now();
    for _ in 0..20 {
        cx.simulate_input("x");
    }
    birchpad_core::timing::within(
        "typing with 1000 carets, a keystroke with a frame",
        started.elapsed() / 20,
        std::time::Duration::from_millis(500),
    );
    assert!(active_text(&workspace, cx).starts_with("somexxxxxxxxxxxxxxxxxxxx text"));
}
