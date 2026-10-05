//! Tests of languages in the application: detection, the Language menu command, `-l`, the
//! large file limit, brace matching commands and auto-indent.

use std::ffi::OsString;

use birchpad_cli::CommandLine;
use birchpad_commands::Invocation;
use birchpad_config::AutoIndent;
use birchpad_core::Range;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use crate::app_state::AppState;
use crate::status_bar::StatusInfo;
use crate::workspace::Workspace;
use crate::workspace::tests::{active_text, document_start, open_workspace, secondary};

fn language(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<&'static str> {
    workspace.read_with(cx, |workspace, cx| {
        let buffer = workspace.active_view(cx).unwrap().read(cx).buffer.read(cx);
        buffer.language().map(|language| language.id)
    })
}

fn has_tree(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    workspace.read_with(cx, |workspace, cx| {
        let buffer = workspace.active_view(cx).unwrap().read(cx).buffer.read(cx);
        buffer
            .syntax()
            .is_some_and(|syntax| syntax.tree().is_some())
    })
}

fn status_language(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> Option<&'static str> {
    workspace.update(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.update(cx, |view, cx| StatusInfo::of(view, cx).language)
    })
}

fn primary(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Range {
    workspace.read_with(cx, |workspace, cx| {
        workspace
            .active_view(cx)
            .unwrap()
            .read(cx)
            .selection
            .primary()
    })
}

fn run(workspace: &Entity<Workspace>, invocation: Invocation, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&invocation, window, cx).unwrap();
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn files_get_their_language_and_a_syntax_tree(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(
        dir.path().join("script"),
        "#!/usr/bin/env python3\nprint(1)\n",
    )
    .unwrap();
    let (workspace, cx) = open_workspace(cx);
    assert_eq!(language(&workspace, cx), None, "untitled is normal text");
    assert_eq!(status_language(&workspace, cx), None);

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(&dir.path().join("main.rs"), window, cx);
    });
    cx.run_until_parked();
    assert_eq!(language(&workspace, cx), Some("rust"));
    assert_eq!(status_language(&workspace, cx), Some("Rust file"));
    assert!(has_tree(&workspace, cx), "parsed in the background");

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(&dir.path().join("script"), window, cx);
    });
    cx.run_until_parked();
    assert_eq!(language(&workspace, cx), Some("python"), "from the #! line");

    // The Language menu overrides detection; "text" is normal text.
    run(
        &workspace,
        Invocation::with_args("language.set", json!({ "language": "bash" })),
        cx,
    );
    assert_eq!(language(&workspace, cx), Some("bash"));
    run(
        &workspace,
        Invocation::with_args("language.set", json!({ "language": "text" })),
        cx,
    );
    assert_eq!(language(&workspace, cx), None);
    assert!(!has_tree(&workspace, cx));
}

#[gpui_kit::test]
fn command_line_language_and_the_large_file_limit(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("notes.txt"), "int main() { return 0; }\n").unwrap();
    let big = "x = 1\n".repeat(200_000);
    std::fs::write(dir.path().join("big.py"), &big).unwrap();
    let (workspace, cx) = open_workspace(cx);
    cx.update(|_, cx| {
        cx.global_mut::<AppState>()
            .settings
            .files
            .large_file_limit_mb = 1
    });

    let args = ["-lcpp", "notes.txt"].map(OsString::from);
    let command_line = CommandLine::parse(args, dir.path());
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_command_line(&command_line, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(language(&workspace, cx), Some("cpp"));

    // Over the limit (1 MB here): plain text, like Notepad++'s Large File Restriction.
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(&dir.path().join("big.py"), window, cx);
    });
    cx.run_until_parked();
    assert_eq!(language(&workspace, cx), None);
    assert!(!has_tree(&workspace, cx));
}

#[gpui_kit::test]
fn brace_commands_go_to_and_select_the_partner(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("f(a[1], b)");
    cx.simulate_keystrokes(document_start());
    cx.simulate_keystrokes("right");
    // The bracket after the caret: `(` at 1 matches `)` at 9.
    cx.simulate_keystrokes(&secondary("b"));
    assert_eq!(primary(&workspace, cx), Range::point(9));
    cx.simulate_keystrokes(&secondary("b"));
    assert_eq!(primary(&workspace, cx), Range::point(1));
    cx.simulate_keystrokes(&secondary("alt-b"));
    assert_eq!(primary(&workspace, cx), Range::new(1, 10));
}

#[gpui_kit::test]
fn enter_indents_like_notepad_plus_plus(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    // New documents use the platform's line ending.
    let eol = birchpad_core::LineEnding::native().as_str();
    let expect = |text: &str| text.replace('\n', eol);
    // Basic: the new line keeps the indentation of the line above.
    cx.update(|_, cx| cx.global_mut::<AppState>().settings.editor.auto_indent = AutoIndent::Basic);
    cx.simulate_input("\tlet x = {");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("y");
    assert_eq!(active_text(&workspace, cx), expect("\tlet x = {\n\ty"));

    // Advanced: one level more after an opening bracket, and Enter between a pair of braces
    // puts the closing one on its own line.
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().settings.editor.auto_indent = AutoIndent::Advanced;
    });
    cx.simulate_keystrokes(&secondary("a"));
    cx.simulate_input("if x {}");
    cx.simulate_keystrokes("left enter");
    cx.simulate_input("body");
    assert_eq!(active_text(&workspace, cx), expect("if x {\n\tbody\n}"));

    // Off: column 1.
    cx.update(|_, cx| cx.global_mut::<AppState>().settings.editor.auto_indent = AutoIndent::Off);
    cx.simulate_keystrokes("enter");
    cx.simulate_input("z");
    assert_eq!(active_text(&workspace, cx), expect("if x {\n\tbody\nz\n}"));
}
