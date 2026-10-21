use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use crate::workspace::{document_start, open_workspace};
use birchpad::status_bar::*;
use birchpad::workspace::Workspace;
use birchpad_commands::Invocation;
use birchpad_core::LineEnding;
use serde_json::json;

fn sections(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> [String; 6] {
    workspace.update(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.update(cx, |view, cx| StatusInfo::of(view, cx))
            .sections()
    })
}

#[gpui_kit::test]
fn an_empty_document_is_one_empty_line(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    assert_eq!(
        sections(&workspace, cx),
        [
            "Length : 0    Lines : 1",
            "Ln : 1    Col : 1    Pos : 1",
            "Sel : 0 | 0",
            &translated_line_ending(LineEnding::native()),
            "UTF-8",
            "INS",
        ]
    );
}

#[test]
fn line_endings_have_notepad_plus_plus_names() {
    assert_eq!(translated_line_ending(LineEnding::CrLf), "Windows (CR LF)");
    assert_eq!(translated_line_ending(LineEnding::Lf), "Unix (LF)");
    assert_eq!(translated_line_ending(LineEnding::Cr), "Macintosh (CR)");
}

#[gpui_kit::test]
fn several_selections_add_up_and_virtual_space_counts_columns(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("abc\nde\nfgh");
    let view = workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap());
    view.update(cx, |view, cx| {
        // "ab" on line 1, and from "e" over the line break to "f" (two lines).
        view.selection = birchpad_core::Selection::new(
            [
                birchpad_core::Range::new(0, 2),
                birchpad_core::Range::new(5, 8),
            ],
            1,
        );
        cx.notify();
    });
    let [_, position, selection, _, _, _] = sections(&workspace, cx);
    assert_eq!(selection, "Sel : 5 | 3");
    assert_eq!(position, "Ln : 3    Col : 2    Pos : 9", "after the f");
    // A caret three columns past the end of "de".
    view.update(cx, |view, cx| {
        view.selection =
            birchpad_core::Selection::single(birchpad_core::Range::point(6).with_virtual(3, 3));
        cx.notify();
    });
    let [_, position, _, _, _, _] = sections(&workspace, cx);
    assert_eq!(position, "Ln : 2    Col : 6    Pos : 7");
}

#[gpui_kit::test]
fn the_language_and_the_format_are_shown(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let info = |cx: &mut VisualTestContext| {
        workspace.update(cx, |workspace, cx| {
            let view = workspace.active_view(cx).unwrap();
            view.update(cx, |view, cx| StatusInfo::of(view, cx))
        })
    };
    assert_eq!(info(cx).language, None, "normal text");
    let run = |invocation: Invocation, cx: &mut VisualTestContext| {
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.dispatch(&invocation, window, cx).unwrap();
        });
        cx.run_until_parked();
    };
    run(
        Invocation::with_args("language.set", json!({ "language": "rust" })),
        cx,
    );
    assert_eq!(info(cx).language.as_deref(), Some("Rust file"));
    run(
        Invocation::with_args("encoding.convert-to", json!({ "encoding": "utf-16be-bom" })),
        cx,
    );
    run(
        Invocation::with_args("edit.convert-eol", json!({ "eol": "cr" })),
        cx,
    );
    let [_, _, _, eol, encoding, _] = info(cx).sections();
    assert_eq!(
        (eol.as_str(), encoding.as_str()),
        ("Macintosh (CR)", "UTF-16 BE BOM")
    );
}

#[gpui_kit::test]
fn the_line_ending_menu_converts(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("a\nb");
    let button = cx.debug_bounds("status-eol").expect("drawn").center();
    cx.simulate_click(button, gpui_kit::Modifiers::none());
    // Windows (CR LF), Unix (LF), Macintosh (CR): the third.
    cx.simulate_keystrokes("down down down enter");
    let format = workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        let buffer = view.read(cx).buffer.read(cx);
        (
            buffer.doc().text().to_string(),
            buffer.doc().format().line_ending,
        )
    });
    assert_eq!(format, ("a\rb".to_owned(), LineEnding::Cr));
}

#[gpui_kit::test]
fn the_encoding_menu_converts_and_escape_chooses_nothing(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("text");
    let encoding = |cx: &mut VisualTestContext| {
        workspace.read_with(cx, |workspace, cx| {
            let view = workspace.active_view(cx).unwrap();
            let format = view.read(cx).buffer.read(cx).doc().format();
            birchpad_io::display_name(format.encoding, format.bom)
        })
    };
    let button = cx.debug_bounds("status-encoding").expect("drawn").center();
    cx.simulate_click(button, gpui_kit::Modifiers::none());
    cx.simulate_keystrokes("down escape");
    assert_eq!(encoding(cx), "UTF-8", "escape chose nothing");

    // Encode in ANSI and the four Unicode forms, Character Sets, then Convert to ANSI,
    // UTF-8, UTF-8-BOM and UTF-16 BE BOM: the tenth item past the separator.
    cx.simulate_click(button, gpui_kit::Modifiers::none());
    cx.simulate_keystrokes(&["down"; 10].join(" "));
    cx.simulate_keystrokes("enter");
    assert_eq!(encoding(cx), "UTF-16 BE BOM");
}

#[gpui_kit::test]
fn shows_what_notepad_plus_plus_shows(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("\tжук\nabc");
    let [length, position, selection, _, encoding, mode] = sections(&workspace, cx);
    assert_eq!(length, "Length : 11    Lines : 2");
    assert_eq!(position, "Ln : 2    Col : 4    Pos : 12");
    assert_eq!(selection, "Sel : 0 | 0");
    assert_eq!((encoding.as_str(), mode.as_str()), ("UTF-8", "INS"));

    // The column counts the tab to its stop; the selection counts characters and lines.
    cx.simulate_keystrokes(&format!("{} end", document_start()));
    let [_, position, _, _, _, _] = sections(&workspace, cx);
    assert_eq!(position, "Ln : 1    Col : 8    Pos : 8");
    cx.simulate_keystrokes("shift-down insert");
    let [_, _, selection, _, _, mode] = sections(&workspace, cx);
    assert_eq!(selection, "Sel : 4 | 2");
    assert_eq!(mode, "OVR");
}
