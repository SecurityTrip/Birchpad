//! Tests of the previews beside the text: where links and images lead, the modes, and the
//! preview following the text.

use std::path::{Path, PathBuf};

use anyhow::Result;
use birchpad_commands::Invocation;
use birchpad_config::PreviewMode;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use super::*;
use crate::app_state::AppState;
use crate::workspace::Workspace;
use crate::workspace::tests::open_workspace;

fn folder() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(r"C:\notes")
    } else {
        PathBuf::from("/notes")
    }
}

#[test]
fn web_links_go_to_the_system_and_paths_to_birchpad() {
    let path = folder();
    let folder = Some(path.as_path());
    assert_eq!(
        link_target("https://example.com/a?b#c", folder),
        LinkTarget::Url("https://example.com/a?b#c".into())
    );
    assert_eq!(
        link_target("mailto:me@example.com", folder),
        LinkTarget::Url("mailto:me@example.com".into())
    );
    assert_eq!(
        link_target("docs/read%20me.md#usage", folder),
        LinkTarget::File(path.join("docs/read me.md"))
    );
    assert_eq!(
        link_target("  other.md  ", folder),
        LinkTarget::File(path.join("other.md")),
        "spaces around the link are not part of it"
    );
}

#[test]
fn links_that_lead_nowhere_are_ignored() {
    let folder = folder();
    assert_eq!(link_target("#usage", Some(&folder)), LinkTarget::None);
    assert_eq!(link_target("", Some(&folder)), LinkTarget::None);
    assert_eq!(
        link_target("other.md", None),
        LinkTarget::None,
        "a relative link of an untitled document"
    );
    assert_eq!(link_target("file://", Some(&folder)), LinkTarget::None);
}

#[test]
fn drives_and_file_urls_are_paths() {
    if cfg!(windows) {
        assert_eq!(
            link_target(r"C:\notes\a.md", None),
            LinkTarget::File(PathBuf::from(r"C:\notes\a.md")),
            "a drive letter is not a scheme"
        );
        assert_eq!(
            link_target("file:///C:/notes/a%2Bb.md", None),
            LinkTarget::File(PathBuf::from("C:/notes/a+b.md"))
        );
    } else {
        assert_eq!(
            link_target("file:///notes/a%2Bb.md", None),
            LinkTarget::File(PathBuf::from("/notes/a+b.md"))
        );
    }
    assert_eq!(
        link_target("x:y", None),
        LinkTarget::None,
        "one letter before the colon is a drive, not a scheme"
    );
}

#[test]
fn images_are_read_from_files_never_from_the_web() {
    let folder = folder();
    assert_eq!(
        image_path("images/logo.png", Some(&folder)),
        Some(folder.join("images/logo.png"))
    );
    assert_eq!(
        image_path("https://example.com/logo.png", Some(&folder)),
        None
    );
    assert_eq!(
        image_path("http://example.com/logo.png", Some(&folder)),
        None
    );
    assert_eq!(image_path("logo.png", None), None, "an untitled document");
    assert_eq!(image_path("", Some(&folder)), None);
}

#[test]
fn escapes_are_decoded_unless_broken() {
    assert_eq!(percent_decode("a%20b%2fc"), "a b/c");
    assert_eq!(
        percent_decode("100%"),
        "100%",
        "an escape cut off at the end"
    );
    assert_eq!(percent_decode("%zz%2"), "%zz%2");
    assert_eq!(percent_decode("%ff"), "%ff", "bytes that are not UTF-8");
    assert_eq!(percent_decode(""), "");
}

#[test]
fn the_menu_checks_the_mode_shown() {
    assert_eq!(
        is_checked("view.preview-split", Some(PreviewMode::Split)),
        Some(true)
    );
    assert_eq!(
        is_checked("view.preview-only", Some(PreviewMode::Split)),
        Some(false)
    );
    assert_eq!(
        is_checked("view.preview-editor", None),
        Some(false),
        "a document without a preview"
    );
    assert_eq!(is_checked("view.word-wrap", Some(PreviewMode::Split)), None);
}

fn run(workspace: &Entity<Workspace>, id: &str, cx: &mut VisualTestContext) -> Result<()> {
    let result = workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&Invocation::new(id), window, cx)
    });
    cx.run_until_parked();
    result
}

fn markdown(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) {
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.dispatch(
                &Invocation::with_args("language.set", json!({ "language": "markdown" })),
                window,
                cx,
            )
        })
        .unwrap();
    cx.run_until_parked();
}

fn mode(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<PreviewMode> {
    workspace.read_with(cx, |workspace, cx| {
        workspace.active_pane().read(cx).preview_mode(cx)
    })
}

fn preview(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<Entity<Preview>> {
    workspace.update(cx, |workspace, cx| {
        let view = workspace.active_view(cx)?;
        workspace
            .active_pane()
            .clone()
            .update(cx, |pane, cx| pane.preview(&view, cx))
    })
}

#[gpui_kit::test]
fn markdown_shows_its_preview_beside_the_text(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    assert_eq!(mode(&workspace, cx), None, "plain text has no preview");
    assert!(preview(&workspace, cx).is_none());
    assert!(cx.debug_bounds("preview-split").is_none());

    markdown(&workspace, cx);
    assert_eq!(
        mode(&workspace, cx),
        Some(PreviewMode::Split),
        "IDEA's default"
    );
    cx.simulate_input("# Title\n\nSome *text*.");
    cx.run_until_parked();
    let shown = preview(&workspace, cx).unwrap();
    let rendered = shown.read_with(cx, |preview, cx| preview.rendered(cx));
    assert!(rendered.contains("Title"), "{rendered:?}");
    assert!(rendered.contains("Some text."), "{rendered:?}");
    assert!(!rendered.contains('*'), "{rendered:?}");

    // The preview follows the text, undo too.
    cx.simulate_input(" More.");
    cx.run_until_parked();
    let rendered = shown.read_with(cx, |preview, cx| preview.rendered(cx));
    assert!(rendered.contains("More."), "{rendered:?}");
}

#[gpui_kit::test]
fn the_modes_are_switched_and_remembered(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    markdown(&workspace, cx);
    run(&workspace, "view.preview-only", cx).unwrap();
    assert_eq!(mode(&workspace, cx), Some(PreviewMode::Preview));
    run(&workspace, "view.preview-editor", cx).unwrap();
    assert_eq!(mode(&workspace, cx), Some(PreviewMode::Editor));
    cx.update(|_, cx| {
        assert_eq!(AppState::global(cx).state.preview, PreviewMode::Editor);
    });

    // A document opened later starts in the mode chosen last; one that has its own keeps it.
    run(&workspace, "file.new", cx).unwrap();
    markdown(&workspace, cx);
    assert_eq!(mode(&workspace, cx), Some(PreviewMode::Editor));
    run(&workspace, "view.preview-split", cx).unwrap();
    run(&workspace, "view.previous-tab", cx).unwrap();
    assert_eq!(mode(&workspace, cx), Some(PreviewMode::Editor));
}

#[gpui_kit::test]
fn a_document_without_a_preview_says_so(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let error = run(&workspace, "view.preview-split", cx).unwrap_err();
    assert!(error.to_string().contains("Markdown"), "{error}");

    // A document that stops being Markdown loses its preview.
    markdown(&workspace, cx);
    assert!(preview(&workspace, cx).is_some());
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.dispatch(
                &Invocation::with_args("language.set", json!({ "language": "rust" })),
                window,
                cx,
            )
        })
        .unwrap();
    assert_eq!(mode(&workspace, cx), None);
    assert!(run(&workspace, "view.preview-only", cx).is_err());
}

#[test]
fn an_empty_folder_path_is_not_a_folder() {
    assert_eq!(
        local_path("a.md", Some(Path::new(""))),
        Some(PathBuf::from("a.md"))
    );
}
