
use birchpad_commands::Invocation;
use birchpad_core::{LineEnding, Range, Selection};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use super::*;
use crate::test_support::Workspace;
use crate::test_support::{
    active_text, document_start, open_workspace, secondary, toggle_bookmark,
};

fn view(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<EditorView> {
    workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap())
}

fn run(workspace: &Entity<Workspace>, invocation: Invocation, cx: &mut VisualTestContext) {
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.dispatch(&invocation, window, cx)
        })
        .unwrap();
    cx.run_until_parked();
}

fn with_style(id: &str, style: usize) -> Invocation {
    Invocation::with_args(id, json!({ "style": style }))
}

fn bookmarks(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<usize> {
    let view = view(workspace, cx);
    view.read_with(cx, |view, cx| view.bookmarked_lines(cx))
}

fn set_bookmarks(workspace: &Entity<Workspace>, lines: &[usize], cx: &mut VisualTestContext) {
    let view = view(workspace, cx);
    view.update(cx, |view, cx| {
        view.update_marks(cx, |marks, text| {
            marks.bookmarks.set_lines(text, lines.iter().copied());
        });
    });
}

fn styled(
    workspace: &Entity<Workspace>,
    style: usize,
    cx: &mut VisualTestContext,
) -> Vec<ByteRange<usize>> {
    let view = view(workspace, cx);
    view.read_with(cx, |view, cx| {
        let marks = view.buffer.read(cx).marks();
        marks.styles[style - 1]
            .iter()
            .map(|(range, _)| range)
            .collect()
    })
}

fn selection(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Range {
    let view = view(workspace, cx);
    view.read_with(cx, |view, _| view.selection.primary())
}

fn select(workspace: &Entity<Workspace>, selection: Selection, cx: &mut VisualTestContext) {
    let view = view(workspace, cx);
    view.update(cx, |view, cx| {
        view.selection = selection;
        cx.notify();
    });
    cx.run_until_parked();
}

fn clipboard(cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
}

/// Jump Up for style `n`: Ctrl+Shift+N, or Cmd+Ctrl+N on macOS.
fn jump_up_key(n: usize) -> String {
    if cfg!(target_os = "macos") {
        format!("cmd-ctrl-{n}")
    } else {
        format!("ctrl-shift-{n}")
    }
}

#[gpui_kit::test]
fn bookmarks_toggle_on_caret_lines_and_navigation_wraps(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("zero\none\ntwo\nthree\nfour");
    cx.simulate_keystrokes(document_start());
    cx.simulate_keystrokes(&format!("down {}", toggle_bookmark()));
    cx.simulate_keystrokes(&format!("down down {}", toggle_bookmark()));
    assert_eq!(bookmarks(&workspace, cx), [1, 3]);

    cx.simulate_keystrokes(document_start());
    cx.simulate_keystrokes("f2");
    assert_eq!(selection(&workspace, cx), Range::point(5), "line 1");
    cx.simulate_keystrokes("f2");
    assert_eq!(selection(&workspace, cx), Range::point(13), "line 3");
    cx.simulate_keystrokes("f2");
    assert_eq!(
        selection(&workspace, cx),
        Range::point(5),
        "wrapped to line 1"
    );
    cx.simulate_keystrokes("shift-f2");
    assert_eq!(
        selection(&workspace, cx),
        Range::point(13),
        "wrapped to line 3"
    );
    cx.simulate_keystrokes("shift-f2");
    assert_eq!(selection(&workspace, cx), Range::point(5));

    // With several carets: marks all their lines unless all are marked, then unmarks them.
    let carets = Selection::new([Range::point(1), Range::point(6), Range::point(7)], 0);
    select(&workspace, carets.clone(), cx);
    cx.simulate_keystrokes(toggle_bookmark());
    assert_eq!(bookmarks(&workspace, cx), [0, 1, 3]);
    cx.simulate_keystrokes(toggle_bookmark());
    assert_eq!(bookmarks(&workspace, cx), [3]);

    run(&workspace, Invocation::new("bookmark.clear-all"), cx);
    assert!(bookmarks(&workspace, cx).is_empty());
    // Without bookmarks, F2 stays put.
    cx.simulate_keystrokes("f2");
    assert_eq!(selection(&workspace, cx), Range::point(1));
}

#[gpui_kit::test]
fn next_bookmark_expands_the_fold_hiding_it(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("fn a() {\n    x();\n}\nfn b() {}\n");
    run(
        &workspace,
        Invocation::with_args("language.set", json!({ "language": "rust" })),
        cx,
    );
    set_bookmarks(&workspace, &[1], cx);
    cx.simulate_keystrokes(document_start());
    run(&workspace, Invocation::new("view.fold-all"), cx);
    let view = view(&workspace, cx);
    assert!(view.read_with(cx, |view, _| view.display.is_hidden(1)));
    cx.simulate_keystrokes("f2");
    assert_eq!(selection(&workspace, cx), Range::point(9));
    assert!(!view.read_with(cx, |view, _| view.display.is_hidden(1)));
}

#[gpui_kit::test]
fn bookmarked_line_operations_are_one_undo_step(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let eol = LineEnding::native().as_str();
    cx.simulate_input("a\nb\nc\nd\ne");
    set_bookmarks(&workspace, &[1, 3], cx);

    run(&workspace, Invocation::new("bookmark.copy-lines"), cx);
    assert_eq!(clipboard(cx), Some(format!("b{eol}d{eol}")));

    run(&workspace, Invocation::new("bookmark.inverse"), cx);
    assert_eq!(bookmarks(&workspace, cx), [0, 2, 4]);
    run(&workspace, Invocation::new("bookmark.inverse"), cx);
    assert_eq!(bookmarks(&workspace, cx), [1, 3]);

    run(&workspace, Invocation::new("bookmark.remove-lines"), cx);
    assert_eq!(active_text(&workspace, cx), "a\nc\ne");
    assert!(
        bookmarks(&workspace, cx).is_empty(),
        "removed with their lines"
    );
    cx.simulate_keystrokes(&secondary("z"));
    assert_eq!(
        active_text(&workspace, cx),
        "a\nb\nc\nd\ne",
        "one undo step"
    );

    set_bookmarks(&workspace, &[0, 4], cx);
    run(
        &workspace,
        Invocation::new("bookmark.remove-unmarked-lines"),
        cx,
    );
    assert_eq!(active_text(&workspace, cx), "a\ne");
    assert_eq!(
        bookmarks(&workspace, cx),
        [0, 1],
        "kept lines keep their bookmarks"
    );
    cx.simulate_keystrokes(&secondary("z"));
    assert_eq!(active_text(&workspace, cx), "a\nb\nc\nd\ne");

    set_bookmarks(&workspace, &[2, 3], cx);
    run(&workspace, Invocation::new("bookmark.cut-lines"), cx);
    assert_eq!(active_text(&workspace, cx), "a\nb\ne");
    assert_eq!(clipboard(cx), Some(format!("c{eol}d{eol}")));
    cx.simulate_keystrokes(&secondary("z"));
    assert_eq!(active_text(&workspace, cx), "a\nb\nc\nd\ne");

    set_bookmarks(&workspace, &[0, 2], cx);
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("X\nY".into())));
    run(&workspace, Invocation::new("bookmark.paste-to-lines"), cx);
    assert_eq!(active_text(&workspace, cx), "X\nY\nb\nX\nY\nd\ne");
    assert_eq!(
        bookmarks(&workspace, cx),
        [0, 3],
        "on the first pasted line"
    );
    cx.simulate_keystrokes(&secondary("z"));
    assert_eq!(active_text(&workspace, cx), "a\nb\nc\nd\ne");
}

#[gpui_kit::test]
fn token_styles_mark_jump_copy_and_clear(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let eol = LineEnding::native().as_str();
    cx.simulate_input("cat dog cat\ncatalog cat");
    cx.simulate_keystrokes(document_start());
    // The word at the caret, whole words only by default.
    cx.simulate_keystrokes(&secondary("alt-1"));
    assert_eq!(styled(&workspace, 1, cx), [0..3, 8..11, 20..23]);
    // A selection, here part of a word: still only whole-word occurrences.
    select(&workspace, Selection::single(Range::new(12, 15)), cx);
    cx.simulate_keystrokes(&secondary("alt-3"));
    assert_eq!(styled(&workspace, 3, cx), [0..3, 8..11, 20..23]);
    cx.update(|_, cx| {
        cx.global_mut::<AppState>()
            .settings
            .highlighting
            .token_style
            .whole_word = false;
    });
    cx.simulate_keystrokes(&secondary("alt-3"));
    assert_eq!(styled(&workspace, 3, cx), [0..3, 8..11, 12..15, 20..23]);
    // Style One Token styles only the token at the caret.
    select(&workspace, Selection::point(5), cx);
    run(&workspace, with_style("mark.style-one", 2), cx);
    assert_eq!(styled(&workspace, 2, cx), [4..7]);

    // Jump Down / Up select the next or previous occurrence, wrapping around.
    select(&workspace, Selection::point(0), cx);
    cx.simulate_keystrokes(&secondary("1"));
    assert_eq!(selection(&workspace, cx), Range::new(8, 11));
    cx.simulate_keystrokes(&secondary("1"));
    assert_eq!(selection(&workspace, cx), Range::new(20, 23));
    cx.simulate_keystrokes(&secondary("1"));
    assert_eq!(selection(&workspace, cx), Range::new(0, 3), "wrapped");
    cx.simulate_keystrokes(&jump_up_key(1));
    assert_eq!(
        selection(&workspace, cx),
        Range::new(20, 23),
        "wrapped back"
    );
    run(&workspace, Invocation::new("mark.jump-up"), cx);
    assert_eq!(selection(&workspace, cx), Range::new(12, 15), "any style");
    run(&workspace, Invocation::new("mark.jump-up"), cx);
    assert_eq!(selection(&workspace, cx), Range::new(8, 11));
    run(&workspace, Invocation::new("mark.jump-up"), cx);
    assert_eq!(selection(&workspace, cx), Range::new(4, 7), "style 2");

    run(&workspace, with_style("mark.copy-styled-text", 2), cx);
    assert_eq!(clipboard(cx), Some(format!("dog{eol}")));
    run(&workspace, Invocation::new("mark.copy-styled-text"), cx);
    assert_eq!(
        clipboard(cx),
        Some(format!("cat{eol}dog{eol}cat{eol}cat{eol}cat{eol}"))
    );

    cx.simulate_keystrokes(&secondary("alt-shift-1"));
    assert!(styled(&workspace, 1, cx).is_empty());
    assert!(!styled(&workspace, 3, cx).is_empty());
    cx.simulate_keystrokes(&secondary("alt-shift-0"));
    assert!(styled(&workspace, 2, cx).is_empty());
    assert!(styled(&workspace, 3, cx).is_empty());
}

#[gpui_kit::test]
fn styles_and_bookmarks_follow_edits_undo_and_redo(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("one two\nthree two");
    cx.simulate_keystrokes(document_start());
    select(&workspace, Selection::point(5), cx);
    cx.simulate_keystrokes(&secondary("alt-1"));
    assert_eq!(styled(&workspace, 1, cx), [4..7, 14..17]);
    set_bookmarks(&workspace, &[1], cx);

    // Typing right before a styled word does not extend it; Enter above moves both.
    cx.simulate_keystrokes(document_start());
    cx.simulate_input("x\n");
    assert_eq!(styled(&workspace, 1, cx), [6..9, 16..19]);
    assert_eq!(bookmarks(&workspace, cx), [2]);
    cx.simulate_keystrokes(&secondary("z"));
    assert_eq!(styled(&workspace, 1, cx), [4..7, 14..17]);
    assert_eq!(bookmarks(&workspace, cx), [1]);
    cx.simulate_keystrokes(&secondary("y"));
    assert_eq!(styled(&workspace, 1, cx), [6..9, 16..19]);
    assert_eq!(bookmarks(&workspace, cx), [2]);
}

/// X11 and Wayland report Ctrl+Alt+Shift+1 as `ctrl-alt-!` and Ctrl+Shift+1 as `ctrl-!`.
#[cfg(target_os = "linux")]
#[gpui_kit::test]
fn shifted_digits_work_as_linux_reports_them(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("cat dog cat");
    cx.simulate_keystrokes(document_start());
    cx.simulate_keystrokes("ctrl-alt-1");
    assert_eq!(styled(&workspace, 1, cx), [0..3, 8..11]);
    cx.simulate_keystrokes("ctrl-!");
    assert_eq!(
        selection(&workspace, cx),
        Range::new(8, 11),
        "Jump Up wraps"
    );
    cx.simulate_keystrokes("ctrl-alt-!");
    assert!(styled(&workspace, 1, cx).is_empty());
}

#[gpui_kit::test]
fn bad_style_numbers_are_reported(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let result = workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&with_style("mark.style-all", 6), window, cx)
    });
    assert!(result.unwrap_err().to_string().contains("style 6"));
}
