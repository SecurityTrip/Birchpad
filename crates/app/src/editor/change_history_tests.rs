//! Tests of the change history (Notepad++'s orange and green lines): edits, saves, undo,
//! reloads, edits made while a comparison runs, the margin and its setting.

use std::fs;
use std::path::Path;

use birchpad_config::ChangeHistory;
use birchpad_core::{Edit, LineChange, Selection, Transaction, UndoGrouping};
use gpui_kit::{Context, Entity, Modifiers, TestAppContext, VisualTestContext, point};

use super::EditorView;
use crate::app_state::AppState;
use crate::buffer::Buffer;
use crate::workspace::Workspace;
use crate::workspace::tests::{active_text, document_start, open_workspace, secondary};

fn open(workspace: &Entity<Workspace>, path: &Path, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(path, window, cx)
    });
    cx.run_until_parked();
}

fn active(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<EditorView> {
    workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap())
}

fn go_to(workspace: &Entity<Workspace>, pos: usize, cx: &mut VisualTestContext) {
    active(workspace, cx).update(cx, |view, cx| {
        view.selection = Selection::point(pos);
        cx.notify();
    });
}

/// The modified and the saved lines of the active document, once the comparison is done.
fn changes(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> (Vec<usize>, Vec<usize>) {
    cx.run_until_parked();
    active(workspace, cx).read_with(cx, |view, cx| {
        let buffer = view.buffer.read(cx);
        let text = buffer.doc().text();
        let (modified, saved) = buffer.changed_lines();
        (modified.lines(text), saved.lines(text))
    })
}

/// A file with `text`, open in the workspace.
fn open_file<'a>(
    text: &str,
    cx: &'a mut TestAppContext,
) -> (
    tempfile::TempDir,
    Entity<Workspace>,
    &'a mut VisualTestContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, text).unwrap();
    let (workspace, cx) = open_workspace(cx);
    open(&workspace, &path, cx);
    (dir, workspace, cx)
}

#[gpui_kit::test]
fn edits_are_modified_until_saved_then_saved(cx: &mut TestAppContext) {
    let (_dir, workspace, cx) = open_file("one\ntwo\nthree\n", cx);
    assert_eq!(changes(&workspace, cx), (vec![], vec![]), "nothing yet");

    go_to(&workspace, 4, cx);
    cx.simulate_input("2 ");
    assert_eq!(changes(&workspace, cx), (vec![1], vec![]));
    cx.simulate_keystrokes(&secondary("s"));
    assert_eq!(changes(&workspace, cx), (vec![], vec![1]), "saved");

    // A line typed above is modified, and moves the saved change down.
    cx.simulate_keystrokes(document_start());
    cx.simulate_input("zero\n");
    assert_eq!(changes(&workspace, cx), (vec![0], vec![2]));
    // A saved line edited again is modified.
    go_to(&workspace, 9, cx);
    cx.simulate_input("!");
    assert_eq!(changes(&workspace, cx), (vec![0, 2], vec![]));
}

#[gpui_kit::test]
fn undoing_back_to_the_saved_text_clears_the_modified_lines(cx: &mut TestAppContext) {
    let (_dir, workspace, cx) = open_file("a\nb\nc\n", cx);
    go_to(&workspace, 2, cx);
    cx.simulate_input("B");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("new");
    assert_eq!(changes(&workspace, cx), (vec![1, 2], vec![]));
    for _ in 0..10 {
        if active_text(&workspace, cx) == "a\nb\nc\n" {
            break;
        }
        cx.simulate_keystrokes(&secondary("z"));
    }
    assert_eq!(active_text(&workspace, cx), "a\nb\nc\n");
    assert_eq!(changes(&workspace, cx), (vec![], vec![]));
    // Redo brings them back.
    cx.simulate_keystrokes(&secondary("y"));
    assert!(!changes(&workspace, cx).0.is_empty());
}

#[gpui_kit::test]
fn deleted_lines_mark_the_line_in_their_place(cx: &mut TestAppContext) {
    let (_dir, workspace, cx) = open_file("a\nb\nc\n", cx);
    // Delete the line "b" with its line break.
    go_to(&workspace, 2, cx);
    cx.simulate_keystrokes("shift-down delete");
    assert_eq!(active_text(&workspace, cx), "a\nc\n");
    assert_eq!(changes(&workspace, cx), (vec![1], vec![]));
}

#[gpui_kit::test]
fn reading_the_file_again_starts_over(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, "one\ntwo\n").unwrap();
    let (workspace, cx) = open_workspace(cx);
    open(&workspace, &path, cx);
    cx.simulate_input("1 ");
    cx.simulate_keystrokes(&secondary("s"));
    assert_eq!(changes(&workspace, cx), (vec![], vec![0]));

    fs::write(&path, "one\ntwo\nthree\n").unwrap();
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.check_files(window, cx)
    });
    cx.run_until_parked();
    cx.simulate_prompt_answer("Reload");
    cx.run_until_parked();
    assert_eq!(active_text(&workspace, cx), "one\ntwo\nthree\n");
    assert_eq!(changes(&workspace, cx), (vec![], vec![]));
}

#[gpui_kit::test]
fn edits_during_a_comparison_are_not_lost(cx: &mut TestAppContext) {
    let (_dir, workspace, cx) = open_file("a\nb\nc\nd\n", cx);
    go_to(&workspace, 2, cx);
    cx.simulate_input("B");
    assert_eq!(changes(&workspace, cx), (vec![1], vec![]));

    // Two edits in a row: the second comes before the comparison of the first has run. Right
    // away the changed line moves with the text; the comparisons then take both edits in.
    let buffer = active(&workspace, cx).read_with(cx, |view, _| view.buffer.clone());
    let moved = buffer.update(cx, |buffer, cx| {
        let insert = |buffer: &mut Buffer, pos: usize, text: &str, cx: &mut Context<Buffer>| {
            let edit = Edit::insert(pos, text);
            let transaction = Transaction::from_edits(buffer.doc().text(), [edit]).unwrap();
            let selection = Selection::point(pos);
            buffer.apply(transaction, &selection, UndoGrouping::NewStep, None, cx);
        };
        insert(buffer, 0, "new\n", cx);
        let before_d = buffer.doc().text().len() - 2;
        insert(buffer, before_d, "y", cx);
        let text = buffer.doc().text();
        buffer.changed_lines().0.lines(text)
    });
    assert_eq!(moved, [2], "the modified line moved down with its text");
    assert_eq!(active_text(&workspace, cx), "new\na\nBb\nc\nyd\n");
    assert_eq!(changes(&workspace, cx), (vec![0, 2, 4], vec![]));
}

#[gpui_kit::test]
fn an_untitled_document_has_its_typed_lines_modified(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("one\ntwo");
    assert_eq!(changes(&workspace, cx), (vec![0, 1], vec![]));
}

#[gpui_kit::test]
fn the_margin_shows_bars_and_the_setting_turns_it_off(cx: &mut TestAppContext) {
    let (_dir, workspace, cx) = open_file("one\ntwo\nthree\n", cx);
    go_to(&workspace, 4, cx);
    cx.simulate_input("2");
    cx.simulate_keystrokes(&secondary("s"));
    cx.simulate_keystrokes(document_start());
    cx.simulate_input("0");
    cx.run_until_parked();
    let bars = |cx: &mut VisualTestContext| {
        active(&workspace, cx).read_with(cx, |view, _| {
            let layout = view.layout.as_ref().expect("a frame was drawn");
            let lines: Vec<_> = layout
                .change_bars
                .iter()
                .map(|(bar, change)| {
                    let row = layout
                        .rows
                        .iter()
                        .find(|row| row.y == bar.top())
                        .expect("a bar beside a row");
                    (row.row.line, *change)
                })
                .collect();
            (lines, layout.margins.changes)
        })
    };
    let (lines, margin) = bars(cx);
    assert_eq!(lines, [(0, LineChange::Modified), (1, LineChange::Saved)]);
    // Between the symbol margin and the folding margin.
    let margin = margin.expect("the change history margin");
    let (symbols, first_row) = active(&workspace, cx).read_with(cx, |view, _| {
        let layout = view.layout.as_ref().unwrap();
        let row = &layout.rows[0];
        (
            layout.margins.symbols.unwrap(),
            row.y + layout.metrics.line_height / 2.,
        )
    });
    assert_eq!(margin.left(), symbols.right());

    // A click in the margin selects the line, as in the line number margin.
    let click = point(margin.left() + margin.size.width / 2., first_row);
    cx.simulate_click(click, Modifiers::none());
    let selection = active(&workspace, cx).read_with(cx, |view, _| view.selection.primary());
    assert_eq!((selection.anchor, selection.head), (0, 5), "the first line");

    cx.update(|_, cx| {
        cx.global_mut::<AppState>().settings.editor.change_history = ChangeHistory::Off;
    });
    active(&workspace, cx).update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    let (lines, margin) = bars(cx);
    assert!(lines.is_empty() && margin.is_none(), "no margin");
}
