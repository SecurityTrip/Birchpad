//! Every command in every kind of document.
//!
//! Each command of the catalog, with the arguments its menus pass, runs on documents in many
//! states: empty, a caret, a selection over lines, several carets, a rectangle into virtual
//! space, CRLF and wide characters, no final line break, word wrap, both views, a language with
//! folds, read-only. None may panic or leave a caret outside the text or inside a CRLF pair, an
//! edit must undo back to the text before it, and nothing may change a read-only document.

use std::path::Path;

use birchpad_cli::CommandLine;
use birchpad_commands::{COMMANDS, Invocation};
use birchpad_config::UserState;
use birchpad_core::{Document, Range, Rope, Selection};
use birchpad_view::{Block, BlockPoint};
use gpui_kit::{ClipboardItem, Entity, TestAppContext, VisualTestContext};
use serde_json::{Value, json};

use super::EditorView;
use crate::app_state::AppState;
use crate::buffer::Buffer;
use crate::commands::CommandRegistry;
use crate::workspace::Workspace;
use crate::workspace::tests::open_workspace;

/// Commands that open a dialog of their own, leave the application or go to the network; their
/// own tests cover them.
const SKIPPED: &[&str] = &[
    "file.open",
    "file.open-folder-as-workspace",
    "file.open-recent",
    "file.save",
    "file.save-as",
    "file.save-all",
    "file.load-session",
    "file.save-session",
    "file.exit",
    "edit.column-editor",
    "search.go-to",
    "help.check-updates",
    "help.update-now",
    "help.restart-to-update",
    "help.about",
];

/// Commands that read the document again rather than edit it: undo does not take them back,
/// and they work in read-only documents too, as in Notepad++.
const REREADS: &[&str] = &[
    // The document's bytes in another encoding.
    "encoding.encode-in",
];

/// The invocations of `id` to try: without arguments, and with each argument its menus and
/// documentation name.
fn invocations(id: &'static str) -> Vec<Invocation> {
    let with = |args: Value| Invocation::with_args(id, args);
    let mut list = vec![Invocation::new(id)];
    match id {
        "edit.insert-text" => list.push(with(json!({ "text": "in\nserted" }))),
        "edit.convert-eol" => {
            for eol in ["crlf", "lf", "cr"] {
                list.push(with(json!({ "eol": eol })));
            }
        }
        "edit.column-insert" => {
            list.push(with(json!({ "text": "|" })));
            list.push(with(json!({
                "initial": 9, "step": 3, "repeat": 2,
                "leading": "zeros", "format": "hex", "uppercase": true
            })));
            list.push(with(
                json!({ "initial": 1, "step": 1, "leading": "spaces", "format": "bin" }),
            ));
            // Boundaries: below zero and counting down through it, the largest number.
            list.push(with(json!({ "initial": -2, "step": 1, "format": "dec" })));
            list.push(with(json!({ "initial": 3, "step": -2, "format": "oct" })));
            list.push(with(json!({ "initial": i64::MAX, "step": 0 })));
        }
        "edit.sort-lines" => {
            let ways = [
                "lexicographic",
                "ignore-case",
                "integer",
                "decimal-comma",
                "decimal-dot",
                "length",
            ];
            for by in ways {
                for descending in [false, true] {
                    list.push(with(json!({ "by": by, "descending": descending })));
                }
            }
        }
        "edit.remove-duplicate-lines" => list.push(with(json!({ "consecutive": true }))),
        "edit.remove-empty-lines" => list.push(with(json!({ "blank": true }))),
        "edit.trim" => {
            for which in ["trailing", "leading", "both"] {
                list.push(with(json!({ "which": which })));
            }
        }
        "edit.eol-to-space" => list.push(with(json!({ "trim": true }))),
        "edit.spaces-to-tabs" => list.push(with(json!({ "leading": true }))),
        "edit.convert-case" => {
            let cases = [
                "upper",
                "lower",
                "proper",
                "proper-blend",
                "sentence",
                "sentence-blend",
                "invert",
                "random",
            ];
            for to in cases {
                list.push(with(json!({ "to": to })));
            }
        }
        "mark.style-all"
        | "mark.style-one"
        | "mark.clear"
        | "mark.jump-up"
        | "mark.jump-down"
        | "mark.copy-styled-text" => {
            for style in [1, 5] {
                list.push(with(json!({ "style": style })));
            }
        }
        "view.fold-level" | "view.unfold-level" => {
            for level in [1, 3, 8] {
                list.push(with(json!({ "level": level })));
            }
        }
        "encoding.encode-in" | "encoding.convert-to" => {
            let encodings = [
                "utf-8",
                "utf-8-bom",
                "utf-16le-bom",
                "utf-16be-bom",
                "ansi",
                "windows-1251",
            ];
            for encoding in encodings {
                list.push(with(json!({ "encoding": encoding })));
            }
        }
        "language.set" => {
            for language in ["rust", "json", "markdown", "html", "python", "text"] {
                list.push(with(json!({ "language": language })));
            }
        }
        "panel.project" => {
            for panel in [1, 3] {
                list.push(with(json!({ "panel": panel })));
            }
        }
        _ => {}
    }
    list
}

/// Invocations of `id` that must be refused without touching the document: a required argument
/// missing, a value it does not know, one past each end of a range, a wrong type.
fn invalid_invocations(id: &'static str) -> Vec<Invocation> {
    let with = |args: Value| Invocation::with_args(id, args);
    match id {
        "edit.insert-text" => vec![Invocation::new(id), with(json!({ "text": 5 }))],
        "edit.convert-eol" => vec![Invocation::new(id), with(json!({ "eol": "nel" }))],
        "edit.column-insert" => vec![
            with(json!({ "initial": 1, "step": 1, "format": "roman" })),
            with(json!({ "repeat": "twice" })),
        ],
        "edit.sort-lines" => vec![with(json!({ "by": "colour" }))],
        "edit.trim" => vec![with(json!({ "which": "middle" }))],
        "edit.convert-case" => vec![Invocation::new(id), with(json!({ "to": "title" }))],
        "mark.style-all"
        | "mark.style-one"
        | "mark.clear"
        | "mark.jump-up"
        | "mark.jump-down"
        | "mark.copy-styled-text" => {
            vec![with(json!({ "style": 0 })), with(json!({ "style": 6 }))]
        }
        "view.fold-level" | "view.unfold-level" => vec![
            Invocation::new(id),
            with(json!({ "level": 0 })),
            with(json!({ "level": 9 })),
        ],
        "encoding.encode-in" | "encoding.convert-to" => {
            vec![Invocation::new(id), with(json!({ "encoding": "klingon" }))]
        }
        "language.set" => vec![Invocation::new(id), with(json!({ "language": "klingon" }))],
        "panel.project" => vec![
            Invocation::new(id),
            with(json!({ "panel": 0 })),
            with(json!({ "panel": 4 })),
        ],
        _ => Vec::new(),
    }
}

/// Lines to sort, trim, join and comment: duplicates, empty and blank lines, numbers, tabs,
/// trailing spaces, mixed case.
const LINES: &str = "banana 10\n  apple 2\t\n\nCherry 1,5\nbanana 10\n\t  \nzeta 0x1F  \nlast line";

/// A document state to run every command in.
struct Scenario {
    text: &'static str,
    /// Opened from a file with `-ro`.
    read_only: bool,
    /// Sets the view up once it shows the text.
    setup: fn(&Entity<Workspace>, &Entity<EditorView>, &mut VisualTestContext),
}

fn no_setup(_: &Entity<Workspace>, _: &Entity<EditorView>, _: &mut VisualTestContext) {}

fn select(view: &Entity<EditorView>, selection: Selection, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| {
        view.selection = selection;
        cx.notify();
    });
}

fn dispatch(workspace: &Entity<Workspace>, invocation: &Invocation, cx: &mut VisualTestContext) {
    // A command may refuse (a missing argument, a read-only document): that is no failure.
    let _ = workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(invocation, window, cx)
    });
    cx.run_until_parked();
}

/// Answers every question with Cancel.
fn answer_prompts(cx: &mut VisualTestContext) {
    while cx.has_pending_prompt() {
        cx.simulate_prompt_answer("Cancel");
        cx.run_until_parked();
    }
}

/// Back to one view of the scenario's document, with the view settings at their defaults.
fn reset(
    workspace: &Entity<Workspace>,
    scenario: &Scenario,
    dir: &Path,
    cx: &mut VisualTestContext,
) -> Entity<EditorView> {
    answer_prompts(cx);
    dispatch(workspace, &Invocation::new("search.close"), cx);
    // Every command starts with the side panels closed, as the window opens.
    workspace.update(cx, |workspace, cx| {
        for kind in crate::panels::PanelKind::ALL {
            workspace.close_panel(kind, cx);
        }
    });
    cx.update(|_, cx| AppState::update_state(cx, |state, _| *state = UserState::default()));
    workspace.update_in(cx, |workspace, window, cx| {
        // Closing the last view leaves an empty "new 1", which opening the scenario's
        // document replaces.
        for view in workspace.all_views(cx) {
            workspace.close_view(&view, window, cx);
        }
        if !scenario.read_only {
            let doc = Document::from_text(Rope::from_str(scenario.text));
            workspace.open_document(doc, window, cx);
        }
    });
    if scenario.read_only {
        std::fs::write(dir.join("read-only.txt"), scenario.text).unwrap();
        let args = ["-ro", "read-only.txt"].map(std::ffi::OsString::from);
        let command_line = CommandLine::parse(args, dir);
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_command_line(&command_line, window, cx);
        });
    }
    cx.run_until_parked();
    let view = workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap());
    assert_eq!(
        view.read_with(cx, |view, cx| view.text(cx).to_string()),
        scenario.text
    );
    (scenario.setup)(workspace, &view, cx);
    cx.run_until_parked();
    workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap())
}

/// Carets of every view are inside the text, on character boundaries, never between the CR
/// and the LF of a line break.
fn check_carets(workspace: &Entity<Workspace>, context: &str, cx: &mut VisualTestContext) {
    workspace.read_with(cx, |workspace, cx| {
        for view in workspace.all_views(cx) {
            let view = view.read(cx);
            let text = view.buffer.read(cx).doc().text();
            assert!(!view.selection.ranges().is_empty(), "{context}: no caret");
            for range in view.selection.ranges() {
                for pos in [range.anchor, range.head] {
                    assert!(
                        pos <= text.len() && text.is_char_boundary(pos),
                        "{context}: caret at {pos} in a text of {} bytes",
                        text.len()
                    );
                    assert!(
                        !(pos > 0
                            && pos < text.len()
                            && text.byte(pos - 1) == b'\r'
                            && text.byte(pos) == b'\n'),
                        "{context}: caret at {pos}, inside a CRLF line break"
                    );
                }
            }
        }
    });
}

fn text_of(buffer: &Entity<Buffer>, cx: &mut VisualTestContext) -> String {
    buffer.read_with(cx, |buffer, _| buffer.doc().text().to_string())
}

/// Runs every command of the catalog in the state `scenario` sets up.
fn sweep(scenario: Scenario, cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (workspace, cx) = open_workspace(cx);
    cx.write_to_clipboard(ClipboardItem::new_string("pasted\ntext".to_owned()));
    let ids: Vec<&'static str> = cx.update(|_, cx| {
        let registry = cx.global::<CommandRegistry>();
        COMMANDS
            .iter()
            .map(|spec| spec.id)
            .filter(|id| !SKIPPED.contains(id) && registry.get(id).is_some())
            .collect()
    });
    let mut refused = Vec::new();
    for id in ids {
        for invocation in invocations(id) {
            let context = if invocation.args.is_null() {
                id.to_owned()
            } else {
                format!("{id} {}", invocation.args)
            };
            let view = reset(&workspace, &scenario, dir.path(), cx);
            let buffer = view.read_with(cx, |view, _| view.buffer.clone());
            let before = text_of(&buffer, cx);

            dispatch(&workspace, &invocation, cx);
            answer_prompts(cx);
            check_carets(&workspace, &context, cx);

            // The document may be closed now; if not, it must be as it was or undo to it.
            let open = workspace.read_with(cx, |workspace, cx| {
                workspace
                    .all_views(cx)
                    .iter()
                    .any(|view| view.read(cx).buffer == buffer)
            });
            if !open {
                continue;
            }
            let after = text_of(&buffer, cx);
            if after == before || REREADS.contains(&id) {
                continue;
            }
            assert!(
                !scenario.read_only,
                "{context}: changed a read-only document"
            );
            for _ in 0..100 {
                if text_of(&buffer, cx) == before {
                    break;
                }
                let undone = buffer.update(cx, |buffer, cx| buffer.undo(None, cx));
                cx.run_until_parked();
                if undone.is_none() {
                    break;
                }
            }
            assert_eq!(
                text_of(&buffer, cx),
                before,
                "{context}: undo does not bring back the text before"
            );
            check_carets(&workspace, &format!("{context}, undone"), cx);
        }
        for invocation in invalid_invocations(id) {
            let context = format!("{id} {} (invalid)", invocation.args);
            let view = reset(&workspace, &scenario, dir.path(), cx);
            let buffer = view.read_with(cx, |view, _| view.buffer.clone());
            let before = text_of(&buffer, cx);
            let result = workspace.update_in(cx, |workspace, window, cx| {
                workspace.dispatch(&invocation, window, cx)
            });
            cx.run_until_parked();
            answer_prompts(cx);
            if result.is_ok() {
                refused.push(format!("{context}: accepted"));
            }
            if text_of(&buffer, cx) != before {
                refused.push(format!("{context}: changed the text"));
            }
            check_carets(&workspace, &context, cx);
        }
    }
    assert!(
        refused.is_empty(),
        "invalid invocations not refused cleanly:\n{}",
        refused.join("\n")
    );
}

#[gpui_kit::test]
fn every_command_in_an_empty_document(cx: &mut TestAppContext) {
    sweep(
        Scenario {
            text: "",
            read_only: false,
            setup: no_setup,
        },
        cx,
    );
}

#[gpui_kit::test]
fn every_command_with_a_caret_in_the_text(cx: &mut TestAppContext) {
    sweep(
        Scenario {
            text: LINES,
            read_only: false,
            setup: |_, view, cx| select(view, Selection::point(14), cx),
        },
        cx,
    );
}

#[gpui_kit::test]
fn every_command_with_a_selection_over_lines(cx: &mut TestAppContext) {
    sweep(
        Scenario {
            text: LINES,
            read_only: false,
            setup: |_, view, cx| select(view, Selection::single(Range::new(3, 40)), cx),
        },
        cx,
    );
}

#[gpui_kit::test]
fn every_command_with_several_carets(cx: &mut TestAppContext) {
    sweep(
        Scenario {
            text: LINES,
            read_only: false,
            setup: |_, view, cx| {
                let ranges = [
                    Range::point(1),
                    Range::new(10, 15),
                    Range::point(LINES.len()),
                ];
                select(view, Selection::new(ranges, 1), cx);
            },
        },
        cx,
    );
}

#[gpui_kit::test]
fn every_command_with_a_rectangle_into_virtual_space(cx: &mut TestAppContext) {
    sweep(
        Scenario {
            text: LINES,
            read_only: false,
            setup: |_, view, cx| {
                view.update(cx, |view, cx| {
                    let block = Block::new(BlockPoint::new(1, 2), BlockPoint::new(4, 14));
                    view.set_block(block, cx);
                });
            },
        },
        cx,
    );
}

#[gpui_kit::test]
fn every_command_with_crlf_and_wide_characters(cx: &mut TestAppContext) {
    const TEXT: &str = "日本語 テキスト\r\nemoji 😀 é and e\u{301}\r\n\tindented\r\n\r\nend";
    sweep(
        Scenario {
            text: TEXT,
            read_only: false,
            setup: |_, view, cx| {
                // From inside the emoji's line to the middle of the next one.
                let start = TEXT.find('😀').unwrap();
                select(view, Selection::single(Range::new(start, start + 22)), cx);
            },
        },
        cx,
    );
}

#[gpui_kit::test]
fn every_command_at_the_end_without_a_final_line_break(cx: &mut TestAppContext) {
    sweep(
        Scenario {
            text: "first\nsecond",
            read_only: false,
            setup: |_, view, cx| select(view, Selection::point(12), cx),
        },
        cx,
    );
}

#[gpui_kit::test]
fn every_command_with_word_wrap(cx: &mut TestAppContext) {
    sweep(
        Scenario {
            text: "a long line of words that wraps, a long line of words that wraps, a long \
                   line of words that wraps, a long line of words that wraps, a long line of \
                   words that wraps, a long line of words that wraps, a long line of words \
                   that wraps\nshort",
            read_only: false,
            setup: |workspace, view, cx| {
                dispatch(workspace, &Invocation::new("view.word-wrap"), cx);
                select(view, Selection::point(150), cx);
            },
        },
        cx,
    );
}

#[gpui_kit::test]
fn every_command_in_both_views(cx: &mut TestAppContext) {
    sweep(
        Scenario {
            text: LINES,
            read_only: false,
            setup: |workspace, view, cx| {
                select(view, Selection::single(Range::new(20, 30)), cx);
                dispatch(workspace, &Invocation::new("view.clone-to-other-view"), cx);
            },
        },
        cx,
    );
}

#[gpui_kit::test]
fn every_command_in_code_with_folds(cx: &mut TestAppContext) {
    sweep(
        Scenario {
            text: "// A sample.\nfn main() {\n    let x = [1, 2];\n    if x.len() > 1 {\n        \
                   println!(\"{x:?}\");\n    }\n}\n\nstruct S {\n    a: u8,\n}\n",
            read_only: false,
            setup: |workspace, view, cx| {
                let rust = Invocation::with_args("language.set", json!({ "language": "rust" }));
                dispatch(workspace, &rust, cx);
                select(view, Selection::point(40), cx);
            },
        },
        cx,
    );
}

#[gpui_kit::test]
fn every_command_in_a_read_only_document(cx: &mut TestAppContext) {
    sweep(
        Scenario {
            text: LINES,
            read_only: true,
            setup: |_, view, cx| select(view, Selection::single(Range::new(3, 40)), cx),
        },
        cx,
    );
}
