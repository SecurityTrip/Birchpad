//! File > Print... (the print preview), Print Now and Export to HTML.

use birchpad::app_state::AppState;
use birchpad::print::*;
use birchpad::workspace::Workspace;
use birchpad_commands::Invocation;
use gpui_kit::component::WindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use crate::workspace::open_workspace;

fn run(
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

fn type_text(workspace: &Entity<Workspace>, text: &str, cx: &mut VisualTestContext) {
    run(
        workspace,
        Invocation::with_args("edit.insert-text", json!({ "text": text })),
        cx,
    )
    .unwrap();
}

/// The print preview opened last.
fn preview() -> Entity<PrintPreview> {
    OPENED
        .with(|opened| opened.borrow().clone().and_then(|weak| weak.upgrade()))
        .expect("the print preview is open")
}

fn printed() -> Vec<(String, usize, bool)> {
    PRINTED.with(|printed| std::mem::take(&mut *printed.borrow_mut()))
}

#[test]
fn the_page_to_print_is_named_after_the_document() {
    let path = print_path("new 1:<x>");
    assert!(path.ends_with("birchpad-print-new_1__x_.html"));
    assert!(
        print_path("").ends_with("birchpad-print-.html"),
        "an empty title"
    );
}

#[test]
fn preferences_become_the_page_setup() {
    let settings = birchpad_config::PrintSettings {
        line_numbers: false,
        colors: birchpad_config::PrintColors::BlackOnWhite,
        paper: birchpad_config::Paper::Letter,
        landscape: true,
        margin_top: 1,
        margin_bottom: 2,
        margin_left: 3,
        margin_right: 4,
        font_size: 0,
        header_left: "$(FILE_NAME)".into(),
        footer_right: "$(CURRENT_PRINTING_PAGE)".into(),
        ..birchpad_config::PrintSettings::default()
    };
    let setup = setup_of(&settings, 8);
    assert_eq!(setup.paper, birchpad_tools::print::Paper::Letter);
    assert!(setup.landscape && !setup.line_numbers);
    assert_eq!(
        (
            setup.margins.top,
            setup.margins.bottom,
            setup.margins.left,
            setup.margins.right
        ),
        (1.0, 2.0, 3.0, 4.0)
    );
    assert_eq!(setup.font_size, 1.0, "a size of 0 is the smallest one");
    assert_eq!(setup.header[0], "$(FILE_NAME)");
    assert_eq!(setup.footer[2], "$(CURRENT_PRINTING_PAGE)");
    assert_eq!(
        setup.colors,
        birchpad_tools::print::PrintColors::BlackOnWhite
    );
    assert_eq!(setup.tab_width, 8);
    // Every color mode and paper has its own.
    for (colors, expected) in [
        (
            birchpad_config::PrintColors::Wysiwyg,
            birchpad_tools::print::PrintColors::Wysiwyg,
        ),
        (
            birchpad_config::PrintColors::Invert,
            birchpad_tools::print::PrintColors::Invert,
        ),
        (
            birchpad_config::PrintColors::NoBackground,
            birchpad_tools::print::PrintColors::NoBackground,
        ),
    ] {
        let settings = birchpad_config::PrintSettings {
            colors,
            ..birchpad_config::PrintSettings::default()
        };
        assert_eq!(setup_of(&settings, 4).colors, expected);
    }
    for (paper, expected) in [
        (birchpad_config::Paper::A3, birchpad_tools::print::Paper::A3),
        (birchpad_config::Paper::A4, birchpad_tools::print::Paper::A4),
        (birchpad_config::Paper::A5, birchpad_tools::print::Paper::A5),
        (
            birchpad_config::Paper::Legal,
            birchpad_tools::print::Paper::Legal,
        ),
    ] {
        let settings = birchpad_config::PrintSettings {
            paper,
            ..birchpad_config::PrintSettings::default()
        };
        assert_eq!(setup_of(&settings, 4).paper, expected);
    }
}

#[gpui_kit::test]
fn print_shows_the_pages_and_prints_them(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let lines: Vec<String> = (1..=150).map(|line| format!("line {line}")).collect();
    type_text(&workspace, &lines.join("\n"), cx);
    run(&workspace, Invocation::new("file.print"), cx).unwrap();
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    let preview = preview();
    let (count, first) = preview.read_with(cx, |preview, _| {
        (
            preview.pages.pages.len(),
            preview.pages.pages[0].rows[0].clone(),
        )
    });
    // 10 points on A4 with 15 mm margins: 63 rows a page.
    assert_eq!(count, 3);
    assert_eq!(first.number, Some(1));
    assert_eq!(first.pieces[0].text, "line 1");
    assert!(!preview.read_with(cx, |preview, _| preview.source.selection));
    // Through the pages and not past either end.
    preview.update(cx, |preview, cx| preview.go(-1, cx));
    assert_eq!(preview.read_with(cx, |preview, _| preview.page), 0);
    preview.update(cx, |preview, cx| preview.go(5, cx));
    assert_eq!(preview.read_with(cx, |preview, _| preview.page), 2);
    // Print... asks for the printer and closes the preview.
    cx.update(|window, cx| preview.update(cx, |preview, cx| preview.print(window, cx)));
    cx.run_until_parked();
    assert_eq!(printed(), [("new 1".to_owned(), 3, true)]);
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
}

#[gpui_kit::test]
fn a_selection_is_printed_alone_with_its_line_numbers(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "one\ntwo\nthree\nfour", cx);
    // Select "two\nthr".
    workspace.update_in(cx, |workspace, _, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.update(cx, |view, _| {
            view.selection =
                birchpad_core::Selection::new(vec![birchpad_core::Range::new(4, 11)], 0);
        });
    });
    run(&workspace, Invocation::new("file.print"), cx).unwrap();
    let preview = preview();
    let rows = preview.read_with(cx, |preview, _| {
        assert!(preview.source.selection);
        preview.pages.pages[0].rows.clone()
    });
    let shown: Vec<(Option<usize>, String)> = rows
        .iter()
        .map(|row| {
            let text = row.pieces.iter().map(|piece| piece.text.as_str()).collect();
            (row.number, text)
        })
        .collect();
    assert_eq!(
        shown,
        [(Some(2), "two".to_owned()), (Some(3), "thr".to_owned())]
    );
}

#[gpui_kit::test]
fn the_preview_follows_preferences(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    type_text(&workspace, "a\nb", cx);
    run(&workspace, Invocation::new("file.print"), cx).unwrap();
    let preview = preview();
    assert_eq!(
        preview.read_with(cx, |preview, _| preview.pages.number_width),
        2
    );
    cx.update(|_, cx| {
        let settings = &mut cx.global_mut::<AppState>().settings.print;
        settings.line_numbers = false;
        settings.header_center = "$(FILE_NAME) $(CURRENT_PRINTING_PAGE)".into();
    });
    preview.update(cx, |preview, cx| preview.refresh(cx));
    let (width, header) = preview.read_with(cx, |preview, _| {
        (
            preview.pages.number_width,
            preview.pages.pages[0].header.clone(),
        )
    });
    assert_eq!(width, 0);
    assert_eq!(header, ["", "new 1 1", ""]);
    // Page Setup... opens Preferences at Print, over the preview.
    cx.update(|window, cx| preview.update(cx, |preview, cx| preview.page_setup(window, cx)));
    cx.run_until_parked();
    let section = birchpad::preferences::OPENED
        .with(|opened| opened.borrow().clone().and_then(|weak| weak.upgrade()))
        .expect("Preferences is open");
    let shown = section.read_with(cx, |preferences, _| preferences.section);
    assert_eq!(birchpad::preferences::sections()[shown], "Print");
}

#[gpui_kit::test]
fn print_now_prints_without_asking(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    printed();
    run(&workspace, Invocation::new("file.print-now"), cx).unwrap();
    assert_eq!(
        printed(),
        [("new 1".to_owned(), 1, false)],
        "an empty document is one page"
    );
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
}

#[gpui_kit::test]
fn without_a_document_there_is_nothing_to_print(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    run(&workspace, Invocation::new("file.close-all"), cx).ok();
    let without = workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).is_none());
    if without {
        assert!(run(&workspace, Invocation::new("file.print"), cx).is_err());
        assert!(run(&workspace, Invocation::new("file.print-now"), cx).is_err());
        assert!(printed().is_empty());
    }
}

#[gpui_kit::test]
fn export_writes_the_page_with_the_highlights(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    run(
        &workspace,
        Invocation::with_args("language.set", json!({ "language": "rust" })),
        cx,
    )
    .unwrap();
    type_text(&workspace, "fn main() {}", cx);
    // Let the syntax tree be parsed.
    for _ in 0..50 {
        cx.run_until_parked();
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("page.html");
    run(
        &workspace,
        Invocation::with_args("file.export-html", json!({ "path": path })),
        cx,
    )
    .unwrap();
    for _ in 0..20 {
        cx.run_until_parked();
    }
    let page = std::fs::read_to_string(&path).unwrap();
    assert!(page.contains("main"));
    assert!(!page.contains("window.print()"));
    let html = workspace.read_with(cx, |workspace, cx| {
        active_html(workspace, false, cx).unwrap().1
    });
    assert!(html.contains("<span style="), "{html}");
    // The preview's pages carry the same highlights.
    let source = workspace.read_with(cx, |workspace, cx| {
        PrintSource::active(workspace, cx).unwrap()
    });
    assert!(!source.spans.is_empty());
}
