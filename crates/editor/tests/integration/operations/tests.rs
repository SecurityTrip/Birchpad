use birchpad_commands::Invocation;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use crate::test_support::Workspace;
use crate::test_support::{active_text, document_start, open_workspace, secondary};

fn dispatch(
    workspace: &Entity<Workspace>,
    invocation: Invocation,
    cx: &mut VisualTestContext,
) -> anyhow::Result<()> {
    let result = workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&invocation, window, cx)
    });
    cx.run_until_parked();
    result
}

/// Move Up Current Line: Ctrl+Shift+Up, or Option+Up on macOS.
fn move_up() -> &'static str {
    if cfg!(target_os = "macos") {
        "alt-up"
    } else {
        "ctrl-shift-up"
    }
}

#[gpui_kit::test]
fn line_operations_from_the_keyboard_undo_in_one_step(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    // Lines that operations create use the document's (here the platform's) line ending.
    let eol = birchpad_core::LineEnding::native().as_str();
    let expect = |text: &str| text.replace('\n', eol);
    cx.simulate_input(&expect("one\ntwo"));
    cx.simulate_keystrokes(&secondary("d"));
    assert_eq!(active_text(&workspace, cx), expect("one\ntwo\ntwo"));
    cx.simulate_keystrokes(move_up());
    assert_eq!(active_text(&workspace, cx), expect("two\none\ntwo"));
    cx.simulate_keystrokes(&secondary("shift-l"));
    assert_eq!(active_text(&workspace, cx), expect("one\ntwo"));
    cx.simulate_keystrokes(&secondary("z"));
    assert_eq!(active_text(&workspace, cx), expect("two\none\ntwo"));

    cx.simulate_keystrokes(&secondary("a"));
    cx.simulate_keystrokes(&secondary("shift-u"));
    assert_eq!(active_text(&workspace, cx), expect("TWO\nONE\nTWO"));
    dispatch(
        &workspace,
        Invocation::with_args("edit.remove-duplicate-lines", json!({})),
        cx,
    )
    .unwrap();
    assert_eq!(active_text(&workspace, cx), expect("TWO\nONE"));
    dispatch(
        &workspace,
        Invocation::with_args("edit.sort-lines", json!({ "by": "lexicographic" })),
        cx,
    )
    .unwrap();
    assert_eq!(active_text(&workspace, cx), expect("ONE\nTWO"));
    cx.simulate_keystrokes(&secondary("z"));
    assert_eq!(
        active_text(&workspace, cx),
        expect("TWO\nONE"),
        "one undo step per operation"
    );
}

#[gpui_kit::test]
fn sorting_numbers_reports_lines_without_one(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("10\nten\n2");
    let sort = Invocation::with_args("edit.sort-lines", json!({ "by": "integer" }));
    let error = dispatch(&workspace, sort, cx).unwrap_err();
    assert!(error.to_string().contains("line 2"), "{error}");
    assert_eq!(active_text(&workspace, cx), "10\nten\n2");
}

#[gpui_kit::test]
fn comments_use_the_language_tokens(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("let x = 1;");
    // Plain text has no comments.
    let toggle = Invocation::new("edit.toggle-comment");
    dispatch(&workspace, toggle.clone(), cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "let x = 1;");

    let rust = Invocation::with_args("language.set", json!({ "language": "rust" }));
    dispatch(&workspace, rust, cx).unwrap();
    dispatch(&workspace, toggle.clone(), cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "// let x = 1;");
    dispatch(&workspace, toggle, cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "let x = 1;");
    cx.simulate_keystrokes(document_start());
    dispatch(&workspace, Invocation::new("edit.block-comment"), cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "/* let x = 1; */");
}
