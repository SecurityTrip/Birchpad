use gpui_kit::{TestAppContext, VisualTestContext};
use serde_json::json;

use crate::workspace::{open_workspace, panes};
use anyhow::Result;
use birchpad::compare::*;
use birchpad::workspace::Workspace;
use birchpad_commands::Invocation;
use birchpad_core::Rope;
use birchpad_tools::compare::Change;
use birchpad_tools::compare::Options;
use gpui_kit::Entity;

fn run(
    workspace: &Entity<Workspace>,
    invocation: Invocation,
    cx: &mut VisualTestContext,
) -> Result<()> {
    let result = workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&invocation, window, cx)
    });
    cx.run_until_parked();
    result
}

fn type_text(workspace: &Entity<Workspace>, text: &str, cx: &mut VisualTestContext) {
    run(
        workspace,
        Invocation::with_args("edit.insert-text", json!({ "text": text })),
        cx,
    )
    .unwrap();
}

/// The compared lines of each view's active document, as (first line, change).
fn marked(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> [Vec<(usize, Change)>; 2] {
    workspace.read_with(cx, |workspace, cx| {
        [0, 1].map(|pane| {
            let Some(view) = workspace.panes()[pane].read(cx).active_item() else {
                return Vec::new();
            };
            let buffer = view.read(cx).buffer.read(cx);
            let text = buffer.doc().text();
            buffer
                .marks()
                .compare
                .iter()
                .map(|(range, change)| (birchpad_core::motion::line_of(text, range.start), *change))
                .collect()
        })
    })
}

fn caret(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> usize {
    workspace.read_with(cx, |workspace, cx| {
        workspace
            .active_view(cx)
            .unwrap()
            .read(cx)
            .selection
            .primary()
            .head
    })
}

#[gpui_kit::test]
fn compares_the_active_document_with_the_tab_before_it(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "a\nb\nc\nd\n", cx);
    run(&workspace, Invocation::new("file.new"), cx).unwrap();
    type_text(&workspace, "a\nB\nc\nd\ne\n", cx);
    run(&workspace, Invocation::new("compare.compare"), cx).unwrap();
    // Side by side, the new document in the other view.
    assert_eq!(
        panes(&workspace, cx).0,
        [vec!["new 1".to_owned()], vec!["new 2".to_owned()]]
    );
    assert_eq!(
        marked(&workspace, cx),
        [
            vec![(1, Change::Changed)],
            vec![(1, Change::Changed), (4, Change::Added)]
        ]
    );
    // The caret is at the first difference; the next ones follow, wrapping around.
    assert_eq!(caret(&workspace, cx), 2);
    run(&workspace, Invocation::new("compare.next"), cx).unwrap();
    assert_eq!(caret(&workspace, cx), 8);
    run(&workspace, Invocation::new("compare.next"), cx).unwrap();
    assert_eq!(caret(&workspace, cx), 2);
    run(&workspace, Invocation::new("compare.previous"), cx).unwrap();
    assert_eq!(caret(&workspace, cx), 8);

    run(&workspace, Invocation::new("compare.clear"), cx).unwrap();
    assert_eq!(marked(&workspace, cx), [vec![], vec![]]);
    assert!(run(&workspace, Invocation::new("compare.next"), cx).is_err());
}

#[gpui_kit::test]
fn the_old_document_shows_what_it_loses(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "keep\ngone\nkeep too", cx);
    run(&workspace, Invocation::new("file.new"), cx).unwrap();
    type_text(&workspace, "keep\nkeep too", cx);
    run(&workspace, Invocation::new("compare.compare"), cx).unwrap();
    assert_eq!(marked(&workspace, cx), [vec![(1, Change::Removed)], vec![]]);
    // Comparing again from the other view: the views are already side by side.
    run(&workspace, Invocation::new("view.focus-other-view"), cx).unwrap();
    run(&workspace, Invocation::new("compare.compare"), cx).unwrap();
    assert_eq!(marked(&workspace, cx), [vec![(1, Change::Added)], vec![]]);
}

#[gpui_kit::test]
fn can_ignore_spaces_and_case(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "if (a)\n", cx);
    run(&workspace, Invocation::new("file.new"), cx).unwrap();
    type_text(&workspace, "IF ( a )\n", cx);
    run(&workspace, Invocation::new("compare.ignore-spaces"), cx).unwrap();
    run(&workspace, Invocation::new("compare.compare"), cx).unwrap();
    assert_eq!(marked(&workspace, cx)[1], [(0, Change::Changed)]);
    run(&workspace, Invocation::new("compare.ignore-case"), cx).unwrap();
    run(&workspace, Invocation::new("compare.compare"), cx).unwrap();
    assert_eq!(marked(&workspace, cx), [vec![], vec![]]);
    let current = cx.update(|_, cx| options(cx));
    assert_eq!(
        is_checked(&Invocation::new("compare.ignore-case"), current),
        Some(true)
    );
    assert_eq!(
        is_checked(&Invocation::new("compare.ignore-spaces"), current),
        Some(true)
    );
    assert_eq!(is_checked(&Invocation::new("file.new"), current), None);
}

#[gpui_kit::test]
fn needs_two_documents(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let error = run(&workspace, Invocation::new("compare.compare"), cx).unwrap_err();
    assert!(error.to_string().contains("second document"), "{error}");
    // The same document in both views is no comparison.
    run(&workspace, Invocation::new("view.clone-to-other-view"), cx).unwrap();
    let error = run(&workspace, Invocation::new("compare.compare"), cx).unwrap_err();
    assert!(error.to_string().contains("same as itself"), "{error}");
}

#[test]
fn marks_whole_lines_on_each_side() {
    let old = Rope::from("a\nb\nc");
    let new = Rope::from("a\nc\nd");
    let differences = birchpad_tools::compare::compare("a\nb\nc", "a\nc\nd", Options::default());
    let [old_marks, new_marks] = marks(&old, &new, &differences);
    assert_eq!(old_marks, [(2..4, Change::Removed)]);
    assert_eq!(new_marks, [(4..5, Change::Added)]);
    assert_eq!(byte_range(&old, &(0..0)), 0..0);
    assert_eq!(byte_range(&old, &(0..3)), 0..5);
}

/// The padding of each view's active document.
fn padded(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> [Vec<(usize, u32)>; 2] {
    workspace.read_with(cx, |workspace, cx| {
        [0, 1].map(|pane| {
            workspace.panes()[pane]
                .read(cx)
                .active_item()
                .map(|view| view.read(cx).display.padding().to_vec())
                .unwrap_or_default()
        })
    })
}

/// What differs within lines, in each view's active document, as text.
fn inline_marked(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> [Vec<String>; 2] {
    workspace.read_with(cx, |workspace, cx| {
        [0, 1].map(|pane| {
            let Some(view) = workspace.panes()[pane].read(cx).active_item() else {
                return Vec::new();
            };
            let buffer = view.read(cx).buffer.read(cx);
            let text = buffer.doc().text();
            buffer
                .marks()
                .compare_inline
                .iter()
                .map(|(range, _)| text.slice(range).to_string())
                .collect()
        })
    })
}

fn set_compare(
    cx: &mut VisualTestContext,
    change: impl FnOnce(&mut birchpad_config::CompareSettings),
) {
    cx.update(|_, cx| {
        change(
            &mut cx
                .global_mut::<birchpad::app_state::AppState>()
                .settings
                .compare,
        );
    });
}

#[gpui_kit::test]
fn aligns_the_lines_and_marks_within_them(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "same\nlet a = 1;\ngone\nend\n", cx);
    run(&workspace, Invocation::new("file.new"), cx).unwrap();
    type_text(&workspace, "same\nlet b = 1;\nend\nmore\nmore\n", cx);
    run(&workspace, Invocation::new("compare.compare"), cx).unwrap();
    // The old side waits for the lines added at the end, the new one for the line removed.
    assert_eq!(padded(&workspace, cx), [vec![(4, 2)], vec![(2, 1)]]);
    assert_eq!(inline_marked(&workspace, cx), [vec!["a"], vec!["b"]]);

    run(&workspace, Invocation::new("compare.clear"), cx).unwrap();
    assert_eq!(padded(&workspace, cx), [vec![], vec![]]);
    assert_eq!(
        inline_marked(&workspace, cx),
        [Vec::<String>::new(), vec![]]
    );
}

#[gpui_kit::test]
fn alignment_and_inline_marks_can_be_turned_off(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    set_compare(cx, |compare| {
        compare.align = false;
        compare.inline = false;
    });
    type_text(&workspace, "x\nlet a = 1;\n", cx);
    run(&workspace, Invocation::new("file.new"), cx).unwrap();
    type_text(&workspace, "let b = 1;\n", cx);
    run(&workspace, Invocation::new("compare.compare"), cx).unwrap();
    assert_eq!(padded(&workspace, cx), [vec![], vec![]]);
    assert_eq!(
        inline_marked(&workspace, cx),
        [Vec::<String>::new(), vec![]]
    );
    // The lines are still marked.
    assert!(!marked(&workspace, cx)[0].is_empty());
}

#[gpui_kit::test]
fn compares_again_after_an_edit(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "a\nb\n", cx);
    run(&workspace, Invocation::new("file.new"), cx).unwrap();
    type_text(&workspace, "a\nb\n", cx);
    run(&workspace, Invocation::new("compare.compare"), cx).unwrap();
    assert_eq!(marked(&workspace, cx), [vec![], vec![]]);
    // Typing a line into the new document: compared again once typing pauses.
    type_text(&workspace, "c\n", cx);
    cx.executor().advance_clock(RECOMPARE_DELAY);
    cx.run_until_parked();
    assert_eq!(marked(&workspace, cx), [vec![], vec![(2, Change::Added)]]);
    assert_eq!(padded(&workspace, cx), [vec![(2, 1)], vec![]]);

    // Turned off, an edit is not compared, though the marks follow it.
    set_compare(cx, |compare| compare.recompare = false);
    type_text(&workspace, "d\n", cx);
    cx.executor().advance_clock(RECOMPARE_DELAY);
    cx.run_until_parked();
    assert_eq!(marked(&workspace, cx), [vec![], vec![(2, Change::Added)]]);
}

#[gpui_kit::test]
fn closing_a_compared_document_ends_the_comparison(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "a\n", cx);
    run(&workspace, Invocation::new("file.new"), cx).unwrap();
    type_text(&workspace, "b\n", cx);
    run(&workspace, Invocation::new("compare.compare"), cx).unwrap();
    // The old document, in the other view, goes; an edit of the new one clears the results.
    run(&workspace, Invocation::new("view.focus-other-view"), cx).unwrap();
    workspace.update_in(cx, |workspace, window, cx| {
        let view = workspace.active_view(cx).unwrap();
        workspace.close_view(&view, window, cx);
    });
    cx.run_until_parked();
    type_text(&workspace, "c\n", cx);
    cx.executor().advance_clock(RECOMPARE_DELAY);
    cx.run_until_parked();
    assert!(cx.update(|_, cx| !cx.has_global::<Comparison>()));
    assert_eq!(marked(&workspace, cx)[0], []);
}

#[gpui_kit::test]
fn compares_with_the_clipboard_in_a_document_that_clear_closes(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "one\ntwo\n", cx);
    let error = run(&workspace, Invocation::new("compare.with-clipboard"), cx).unwrap_err();
    assert!(error.to_string().contains("clipboard"), "{error}");
    cx.update(|_, cx| {
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("one\n2\n".into()))
    });
    run(&workspace, Invocation::new("compare.with-clipboard"), cx).unwrap();
    let (names, active) = panes(&workspace, cx);
    assert_eq!(names[1], ["Clipboard"]);
    assert_eq!(active, 0, "the compared document stays active");
    assert_eq!(
        marked(&workspace, cx),
        [vec![(1, Change::Changed)], vec![(1, Change::Changed)]]
    );
    // The clipboard's document is read-only, and goes with the results.
    let read_only = workspace.read_with(cx, |workspace, cx| {
        let view = workspace.panes()[1].read(cx).active_item().unwrap().clone();
        view.read(cx).buffer.read(cx).read_only().is_some()
    });
    assert!(read_only);
    run(&workspace, Invocation::new("compare.clear"), cx).unwrap();
    assert!(panes(&workspace, cx).0[1].is_empty());
    assert_eq!(marked(&workspace, cx)[0], []);
}

#[gpui_kit::test]
fn compares_with_the_last_save(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    // An untitled document has no save to compare with.
    let error = run(&workspace, Invocation::new("compare.with-saved"), cx).unwrap_err();
    assert!(error.to_string().contains("save it first"), "{error}");
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    std::fs::write(&file, "kept\nold\n").unwrap();
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(&file, window, cx)
    });
    cx.run_until_parked();
    run(&workspace, Invocation::new("edit.select-all"), cx).unwrap();
    type_text(&workspace, "kept\nnew\nadded\n", cx);
    run(&workspace, Invocation::new("compare.with-saved"), cx).unwrap();
    assert_eq!(panes(&workspace, cx).0[1], ["notes.txt (last save)"]);
    assert_eq!(
        marked(&workspace, cx),
        [vec![(1, Change::Changed)], vec![(1, Change::Changed)]]
    );
    // A file gone from the disk: said so, nothing opened.
    run(&workspace, Invocation::new("compare.clear"), cx).unwrap();
    std::fs::remove_file(&file).unwrap();
    let error = run(&workspace, Invocation::new("compare.with-saved"), cx).unwrap_err();
    assert!(format!("{error:#}").contains("notes.txt"), "{error:#}");
    assert!(panes(&workspace, cx).0[1].is_empty());
}

/// Runs git in `dir`, for a repository to compare with.
fn git(dir: &std::path::Path, args: &[&str]) -> bool {
    std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .is_ok_and(|output| output.status.success())
}

#[gpui_kit::test]
fn compares_with_git_head(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("tracked.txt");
    std::fs::write(&file, "first\nsecond\n").unwrap();
    let made = git(dir.path(), &["init", "-q"])
        && git(dir.path(), &["add", "tracked.txt"])
        && git(
            dir.path(),
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-q",
                "-m",
                "first",
            ],
        );
    if !made {
        eprintln!("git is not available: skipped");
        return;
    }
    std::fs::write(&file, "first\nsecond\nthird\n").unwrap();
    let (workspace, cx) = open_workspace(cx);
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(&file, window, cx)
    });
    cx.run_until_parked();
    run(&workspace, Invocation::new("compare.with-git"), cx).unwrap();
    assert_eq!(panes(&workspace, cx).0[1], ["tracked.txt (HEAD)"]);
    assert_eq!(marked(&workspace, cx), [vec![], vec![(2, Change::Added)]]);

    // A file Git does not track: what Git says.
    run(&workspace, Invocation::new("compare.clear"), cx).unwrap();
    let untracked = dir.path().join("untracked.txt");
    std::fs::write(&untracked, "x").unwrap();
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(&untracked, window, cx)
    });
    cx.run_until_parked();
    let error = run(&workspace, Invocation::new("compare.with-git"), cx).unwrap_err();
    assert!(error.to_string().contains("HEAD"), "{error}");
}

#[test]
fn git_head_says_what_went_wrong() {
    let dir = tempfile::tempdir().unwrap();
    // Not a repository: Git refuses, or cannot be run at all.
    let error = git_head(dir.path(), "missing.txt").unwrap_err();
    assert!(error.to_string().contains("Git"), "{error}");
}

#[gpui_kit::test]
fn marks_the_differences_along_the_scrollbar(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, &"same\n".repeat(200), cx);
    run(&workspace, Invocation::new("file.new"), cx).unwrap();
    let mut text = "same\n".repeat(200);
    text.replace_range(0..4, "diff");
    type_text(&workspace, &text, cx);
    run(&workspace, Invocation::new("compare.compare"), cx).unwrap();
    let markers = |cx: &mut VisualTestContext| {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        workspace.read_with(cx, |workspace, cx| {
            let view = workspace.active_view(cx).unwrap();
            view.read(cx)
                .layout
                .as_ref()
                .and_then(|layout| layout.vertical_bar.as_ref())
                .map(|bar| bar.markers.len())
                .unwrap_or(0)
        })
    };
    assert_eq!(markers(cx), 1);
    set_compare(cx, |compare| compare.navigation_bar = false);
    assert_eq!(markers(cx), 0);
}
