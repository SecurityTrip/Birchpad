use birchpad_core::{Range, Selection};
use gpui_kit::{TestAppContext, VisualTestContext};

use super::*;
use crate::workspace::tests::{active_text, document_start, open_workspace, secondary};

fn search_for(
    workspace: &Entity<Workspace>,
    pattern: &str,
    options: FindOptions,
    cx: &mut VisualTestContext,
) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.find_bar.update(cx, |bar, cx| {
            bar.show(FindTab::Replace, Some(pattern.to_owned()), None, window, cx);
            bar.options = options;
        });
    });
}

fn set_replacement(workspace: &Entity<Workspace>, text: &str, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.find_bar.update(cx, |bar, cx| {
            bar.replace_input
                .update(cx, |input, cx| input.set_value(text, window, cx));
        });
    });
}

fn act(
    workspace: &Entity<Workspace>,
    event: FindBarEvent,
    cx: &mut VisualTestContext,
) -> Option<String> {
    workspace.update_in(cx, |workspace, window, cx| {
        let bar = workspace.find_bar.clone();
        workspace.on_find_bar_event(&bar, &event, window, cx);
        bar.read(cx).status.clone().map(|(message, _)| message)
    })
}

fn selection(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> (usize, usize) {
    workspace.read_with(cx, |workspace, cx| {
        let range = workspace
            .active_view(cx)
            .unwrap()
            .read(cx)
            .selection
            .primary();
        (range.anchor, range.head)
    })
}

fn select(workspace: &Entity<Workspace>, anchor: usize, head: usize, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, _, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.update(cx, |view, cx| {
            view.selection = Selection::single(Range::new(anchor, head));
            cx.notify();
        });
    });
}

fn regex() -> FindOptions {
    FindOptions {
        mode: SearchMode::Regex,
        match_case: true,
        ..FindOptions::default()
    }
}

#[gpui_kit::test]
fn f3_finds_next_and_wraps(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("cat Cat category cat");
    cx.simulate_keystrokes(document_start());
    let options = FindOptions {
        whole_word: true,
        ..FindOptions::default()
    };
    search_for(&workspace, "cat", options, cx);
    cx.simulate_keystrokes("f3");
    assert_eq!(selection(&workspace, cx), (0, 3));
    cx.simulate_keystrokes("f3");
    assert_eq!(selection(&workspace, cx), (4, 7), "case-insensitive");
    cx.simulate_keystrokes("f3");
    assert_eq!(
        selection(&workspace, cx),
        (17, 20),
        "whole word skips category"
    );
    cx.simulate_keystrokes("f3");
    assert_eq!(selection(&workspace, cx), (0, 3), "wrapped around");
    cx.simulate_keystrokes("shift-f3");
    assert_eq!(selection(&workspace, cx), (17, 20), "backward wraps too");
}

#[gpui_kit::test]
fn replace_all_is_one_undo_step(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("a-b-c-d");
    search_for(&workspace, "-", FindOptions::default(), cx);
    set_replacement(&workspace, "+", cx);
    let status = act(&workspace, FindBarEvent::ReplaceAll, cx);
    assert_eq!(active_text(&workspace, cx), "a+b+c+d");
    assert_eq!(
        status.as_deref(),
        Some("Replace All: 3 occurrences replaced in entire file")
    );
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.close_find(window, cx);
    });
    cx.simulate_keystrokes(&secondary("z"));
    assert_eq!(active_text(&workspace, cx), "a-b-c-d");
}

#[gpui_kit::test]
fn regular_expressions_replace_with_groups(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("a=1, b=22");
    search_for(&workspace, r"(\w)=(\d+)", regex(), cx);
    set_replacement(&workspace, r"$2:\U$1", cx);
    act(&workspace, FindBarEvent::ReplaceAll, cx);
    assert_eq!(active_text(&workspace, cx), "1:A, 22:B");

    // Replace replaces the selected match with its own groups, then finds the next one.
    cx.simulate_keystrokes(document_start());
    search_for(&workspace, r"(\d+):(\w)", regex(), cx);
    set_replacement(&workspace, "$2", cx);
    cx.simulate_keystrokes("f3");
    assert_eq!(selection(&workspace, cx), (0, 3));
    act(&workspace, FindBarEvent::Replace, cx);
    assert_eq!(active_text(&workspace, cx), "A, 22:B");
    assert_eq!(selection(&workspace, cx), (3, 7), "the next match");
}

#[gpui_kit::test]
fn in_selection_limits_replace_all_and_count(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("x x x\nx x x");
    let in_selection = FindOptions {
        in_selection: true,
        ..FindOptions::default()
    };
    search_for(&workspace, "x", in_selection, cx);
    select(&workspace, 6, 11, cx);
    let status = act(&workspace, FindBarEvent::Count, cx);
    assert_eq!(status.as_deref(), Some("Count: 3 matches in selection"));
    set_replacement(&workspace, "y", cx);
    act(&workspace, FindBarEvent::ReplaceAll, cx);
    assert_eq!(active_text(&workspace, cx), "x x x\ny y y");

    // Opening the panel with a selection over several lines turns In selection on.
    select(&workspace, 0, 11, cx);
    cx.simulate_keystrokes(&secondary("h"));
    let options = workspace.read_with(cx, |workspace, cx| workspace.find_bar.read(cx).options);
    assert!(options.in_selection);
}

#[gpui_kit::test]
fn find_next_steps_over_empty_matches(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("x\ny\nz");
    cx.simulate_keystrokes(document_start());
    search_for(&workspace, "^", regex(), cx);
    let mut starts = Vec::new();
    for _ in 0..4 {
        cx.simulate_keystrokes("f3");
        starts.push(selection(&workspace, cx).0);
    }
    assert_eq!(starts, [2, 4, 0, 2], "line starts, wrapping around");
    let status = act(&workspace, FindBarEvent::Count, cx);
    assert_eq!(status.as_deref(), Some("Count: 3 matches in entire file"));
}

#[gpui_kit::test]
fn extended_mode_finds_line_breaks(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("a\tb");
    cx.simulate_keystrokes(document_start());
    let extended = FindOptions {
        mode: SearchMode::Extended,
        ..FindOptions::default()
    };
    search_for(&workspace, r"a\tb", extended, cx);
    cx.simulate_keystrokes("f3");
    assert_eq!(selection(&workspace, cx), (0, 3));
    search_for(&workspace, "(", regex(), cx);
    let status = act(&workspace, FindBarEvent::Count, cx);
    assert!(
        status
            .as_deref()
            .is_some_and(|status| status.starts_with("Find: invalid regular expression")),
        "{status:?}"
    );
}

#[gpui_kit::test]
fn mark_all_marks_bookmarks_purges_and_copies(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("foo bar\nbaz foo\nqux");
    let marking = FindOptions {
        bookmark_line: true,
        ..FindOptions::default()
    };
    search_for(&workspace, "foo", marking, cx);
    let status = act(&workspace, FindBarEvent::MarkAll, cx);
    assert_eq!(status.as_deref(), Some("Mark: 2 matches in entire file"));
    let marks = |cx: &mut VisualTestContext| {
        workspace.read_with(cx, |workspace, cx| {
            let view = workspace.active_view(cx).unwrap();
            let buffer = view.read(cx).buffer.read(cx);
            let found: Vec<_> = buffer.marks().found.iter().map(|(r, _)| r).collect();
            (found, buffer.bookmark_lines())
        })
    };
    assert_eq!(marks(cx), (vec![0..3, 12..15], vec![0, 1]));

    let line_ending = workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.read(cx).buffer.read(cx).doc().line_ending().as_str()
    });
    act(&workspace, FindBarEvent::CopyMarked, cx);
    let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(copied, Some(format!("foo{line_ending}foo{line_ending}")));

    // Purge for each search: the next Mark All replaces the marks.
    let purging = FindOptions {
        purge: true,
        ..FindOptions::default()
    };
    search_for(&workspace, "ba", purging, cx);
    act(&workspace, FindBarEvent::MarkAll, cx);
    assert_eq!(marks(cx).0, [4..6, 8..10]);
    act(&workspace, FindBarEvent::ClearMarks, cx);
    assert!(marks(cx).0.is_empty());
}

#[gpui_kit::test]
fn replace_all_in_all_opened_documents(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("a a");
    cx.simulate_keystrokes(&secondary("n"));
    cx.simulate_input("a");
    search_for(&workspace, "a", FindOptions::default(), cx);
    set_replacement(&workspace, "b", cx);
    let status = act(&workspace, FindBarEvent::ReplaceAllInAll, cx);
    assert_eq!(
        status.as_deref(),
        Some("Replace All in all opened documents: 3 occurrences replaced in 2 documents")
    );
    let texts = workspace.read_with(cx, |workspace, cx| {
        workspace
            .all_views(cx)
            .iter()
            .map(|view| view.read(cx).text(cx).to_string())
            .collect::<Vec<_>>()
    });
    assert_eq!(texts, ["b b", "b"]);
}

#[gpui_kit::test]
fn select_and_find_next_takes_the_word_at_the_caret(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("foo bar foo");
    cx.simulate_keystrokes(document_start());
    cx.simulate_keystrokes("right");
    cx.simulate_keystrokes(&secondary("f3"));
    assert_eq!(selection(&workspace, cx), (8, 11));
    cx.simulate_keystrokes(&secondary("shift-f3"));
    assert_eq!(selection(&workspace, cx), (0, 3));
}

#[gpui_kit::test]
fn find_all_lists_lines_and_f4_walks_them(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("foo\nbar foo\nfoo foo");
    cx.simulate_keystrokes(&secondary("n"));
    cx.simulate_input("no match\nfoo");
    search_for(&workspace, "foo", FindOptions::default(), cx);
    let status = act(
        &workspace,
        FindBarEvent::FindAll {
            all_documents: true,
        },
        cx,
    );
    assert_eq!(status.as_deref(), Some("Find All: 5 hits"));
    let rows = workspace.read_with(cx, |workspace, cx| {
        workspace.search_results.read(cx).rows_text()
    });
    assert_eq!(
        rows,
        [
            "Search \"foo\" (5 hits in 2 files of 2 searched)",
            "  new 2 (1)",
            "    Line 2: foo",
            "  new 1 (4)",
            "    Line 1: foo",
            "    Line 2: bar foo",
            "    Line 3: foo foo",
        ],
        "the active document first, one row per line"
    );
    let active = |cx: &mut VisualTestContext| {
        workspace.read_with(cx, |workspace, cx| {
            let view = workspace.active_view(cx).unwrap();
            let view = view.read(cx);
            let range = view.selection.primary();
            (
                view.buffer.read(cx).display_name(),
                range.anchor,
                range.head,
            )
        })
    };
    cx.simulate_keystrokes("f4");
    assert_eq!(active(cx), ("new 2".to_owned(), 9, 12));
    cx.simulate_keystrokes("f4");
    assert_eq!(active(cx), ("new 1".to_owned(), 0, 3), "into the other tab");
    cx.simulate_keystrokes("f4 f4");
    assert_eq!(
        active(cx),
        ("new 1".to_owned(), 12, 15),
        "the first match of the line"
    );
    cx.simulate_keystrokes("shift-f4");
    assert_eq!(active(cx), ("new 1".to_owned(), 8, 11));
}
