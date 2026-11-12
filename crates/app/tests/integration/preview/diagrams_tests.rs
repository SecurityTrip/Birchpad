//! Tests of the PlantUML preview (ADR 0057): its runs, stopped when the text moves on, and the
//! mistakes PlantUML finds, placed on the lines of the document.

use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui_kit::{
    Entity, Modifiers, MouseButton, Pixels, Point, ScrollDelta, ScrollEvent, TestAppContext,
    VisualTestContext, point, px,
};

use crate::workspace::open_workspace;
use birchpad::app_state::AppState;
use birchpad::preview::*;
use birchpad::workspace::Workspace;
use birchpad_config::DiagramBackground;

#[gpui_kit::test]
fn the_crash_links_to_graphviz_and_draws_again_once_it_is_found(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    DOT.with(|dot| *dot.borrow_mut() = Some(None));
    let dir = tempfile::tempdir().unwrap();
    let plantuml = fake_plantuml(
        dir.path(),
        "echo ^<svg^>crash^</svg^>\n>&2 echo java.lang.NullPointerException\n\
         >&2 echo 	at a.SmetanaEdge.draw(SmetanaEdge.java:1)\nexit /b 1",
        "echo '<svg>crash</svg>'\necho java.lang.NullPointerException >&2\n\
         printf '\\tat a.SmetanaEdge.draw(SmetanaEdge.java:1)\\n' >&2\nexit 1",
    );
    let shown = draw(
        &workspace,
        &plantuml,
        dir.path(),
        "large.puml",
        "@startuml\nA -> B\n@enduml\n",
        cx,
    );
    assert!(shown.read_with(cx, |preview, _| preview.waiting_for_graphviz()));
    // The links: Graphviz's download page, and Preferences > Preview.
    birchpad::file_commands::SHELL.with(|shell| shell.borrow_mut().clear());
    run(&workspace, "preview.get-graphviz", cx).unwrap();
    let asked = birchpad::file_commands::SHELL.with(|shell| shell.borrow().clone());
    assert_eq!(
        asked,
        [birchpad::file_commands::Shell::Browse(
            GRAPHVIZ_DOWNLOAD.to_owned()
        )]
    );
    run(&workspace, "preview.choose-layout", cx).unwrap();
    let preferences = birchpad::preferences::OPENED
        .with(|opened| opened.borrow().clone().and_then(|weak| weak.upgrade()))
        .expect("Preferences is open");
    let section = preferences.read_with(cx, |preferences, _| preferences.section);
    assert_eq!(birchpad::preferences::sections()[section], "Preview");
    // Still no Graphviz: it keeps looking.
    cx.executor().advance_clock(DOT_POLL);
    cx.run_until_parked();
    assert!(shown.read_with(cx, |preview, _| preview.waiting_for_graphviz()));
    // Graphviz installed: drawn again with it, and no longer waiting.
    let dot = dir.path().join("dot");
    DOT.with(|found| *found.borrow_mut() = Some(Some(dot.clone())));
    cx.executor().advance_clock(DOT_POLL);
    cx.run_until_parked();
    for _ in 0..500 {
        if shown.read_with(cx, |preview, _| preview.layout()) == "Graphviz" {
            break;
        }
        cx.executor().advance_clock(PLANTUML_DELAY);
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        shown.read_with(cx, |preview, _| preview.layout()),
        "Graphviz"
    );
    assert!(!shown.read_with(cx, |preview, _| preview.waiting_for_graphviz()));
    DOT.with(|found| *found.borrow_mut() = None);
}

#[gpui_kit::test]
fn the_preview_draws_with_the_layout_of_the_setting(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    // The diagram is the arguments PlantUML was given after the diagram's end (cmd splits
    // `-Playout=smetana` in two).
    let plantuml = fake_plantuml(
        dir.path(),
        "echo ^<svg^>%7 %8^</svg^>",
        "echo \"<svg>$7 $8</svg>\"",
    );
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().settings.preview.plantuml_layout =
            birchpad_config::DiagramLayout::Smetana;
    });
    let shown = draw(
        &workspace,
        &plantuml,
        dir.path(),
        "flow.puml",
        "@startuml\nA -> B\n@enduml\n",
        cx,
    );
    let svg =
        String::from_utf8(shown.read_with(cx, |preview, _| preview.svgs()).remove(0)).unwrap();
    assert!(svg.contains("-Playout") && svg.contains("smetana"), "{svg}");
    assert_eq!(
        shown.read_with(cx, |preview, _| preview.layout()),
        "smetana"
    );
}

#[gpui_kit::test]
fn the_preview_folds_a_crash(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let plantuml = fake_plantuml(
        dir.path(),
        "echo ^<svg^>crash^</svg^>\n>&2 echo java.lang.NullPointerException\n\
         >&2 echo 	at a.SmetanaEdge.draw(SmetanaEdge.java:1)\nexit /b 1",
        "echo '<svg>crash</svg>'\necho java.lang.NullPointerException >&2\n\
         printf '\\tat a.SmetanaEdge.draw(SmetanaEdge.java:1)\\n' >&2\nexit 1",
    );
    let shown = draw(
        &workspace,
        &plantuml,
        dir.path(),
        "flow.puml",
        "@startuml\nA -> B\n@enduml\n",
        cx,
    );
    let found = shown.read_with(cx, |preview, _| preview.mistakes());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].message, "java.lang.NullPointerException");
    assert!(found[0].details.is_some());
    assert!(
        !shown.read_with(cx, |preview, _| preview.details_shown()),
        "folded"
    );
    shown.update(cx, |preview, cx| preview.toggle_details(cx));
    assert!(shown.read_with(cx, |preview, _| preview.details_shown()));
    cx.run_until_parked();
    shown.update(cx, |preview, cx| preview.toggle_details(cx));
    assert!(!shown.read_with(cx, |preview, _| preview.details_shown()));
}

/// A script standing in for PlantUML, `windows` its body as a `.cmd` file, `unix` as a shell
/// script.
pub(crate) fn fake_plantuml(dir: &Path, windows: &str, unix: &str) -> PathBuf {
    if cfg!(windows) {
        let path = dir.join("fake-plantuml.cmd");
        std::fs::write(
            &path,
            format!("@echo off\r\n{}\r\n", windows.replace('\n', "\r\n")),
        )
        .unwrap();
        path
    } else {
        let path = dir.join("fake-plantuml");
        std::fs::write(&path, format!("#!/bin/sh\ncat > /dev/null\n{unix}\n")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }
}

fn preview(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<Preview> {
    workspace.update(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        workspace
            .active_pane()
            .clone()
            .update(cx, |pane, cx| pane.preview(&view, cx))
            .unwrap()
    })
}

/// Opens `name` with `text` in `dir`, drawn by `plantuml`, and waits until the preview shows
/// what it drew: the preview.
pub(crate) fn draw(
    workspace: &Entity<Workspace>,
    plantuml: &Path,
    dir: &Path,
    name: &str,
    text: &str,
    cx: &mut VisualTestContext,
) -> Entity<Preview> {
    let setting = plantuml.display().to_string();
    cx.update(|_, cx| cx.global_mut::<AppState>().settings.preview.plantuml = setting);
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(&path, window, cx);
    });
    let shown = preview(workspace, cx);
    // The drawing runs a program outside the test executor.
    for _ in 0..500 {
        cx.run_until_parked();
        let rendered = shown.read_with(cx, |preview, cx| preview.rendered(cx));
        if rendered.ends_with(" diagrams") && !rendered.starts_with("0 ") {
            return shown;
        }
        cx.executor().advance_clock(PLANTUML_DELAY);
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("PlantUML drew nothing");
}

#[gpui_kit::test]
fn the_preview_lists_mistakes_and_goes_to_their_lines(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let plantuml = fake_plantuml(
        dir.path(),
        "echo ^<svg^>^<text^>Syntax Error?^</text^>^</svg^>\n\
         >&2 echo ERROR\n>&2 echo 1\n>&2 echo Syntax Error?\nexit /b 200",
        "echo '<svg><text>Syntax Error?</text></svg>'\n\
         echo ERROR >&2\necho 1 >&2\necho 'Syntax Error?' >&2\nexit 200",
    );
    let shown = draw(
        &workspace,
        &plantuml,
        dir.path(),
        "flow.puml",
        "' title\n@startuml\nqux\n@enduml\n",
        cx,
    );
    let found = shown.read_with(cx, |preview, _| preview.mistakes());
    assert_eq!(
        found,
        [Mistake {
            line: Some(2),
            message: "Syntax Error?".into(),
            included: None,
            details: None,
        }]
    );
    cx.update(|window, cx| Preview::go_to_line(&shown, 2, window, cx));
    let caret = workspace.read_with(cx, |workspace, cx| {
        workspace
            .active_view(cx)
            .unwrap()
            .read(cx)
            .selection
            .primary()
            .head
    });
    assert_eq!(caret, "' title\n@startuml\n".len());
}

#[gpui_kit::test]
fn a_run_that_succeeds_has_no_mistakes(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    // Java's warnings on the error output of a run that drew.
    let plantuml = fake_plantuml(
        dir.path(),
        "echo ^<svg^>^</svg^>\n>&2 echo WARNING: something",
        "echo '<svg></svg>'\necho 'WARNING: something' >&2",
    );
    let shown = draw(
        &workspace,
        &plantuml,
        dir.path(),
        "flow.puml",
        "@startuml\nA -> B\n@enduml\n",
        cx,
    );
    assert!(
        shown
            .read_with(cx, |preview, _| preview.mistakes())
            .is_empty()
    );
    assert_eq!(
        shown.read_with(cx, |preview, cx| preview.rendered(cx)),
        "1 diagrams"
    );
}

#[gpui_kit::test]
fn plantuml_runs_in_the_documents_folder(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    // The diagram is the folder PlantUML runs in, where `!include` looks for files.
    let plantuml = fake_plantuml(
        dir.path(),
        "echo ^<svg^>%CD%^</svg^>",
        "echo \"<svg>$(pwd)</svg>\"",
    );
    let folder = dir.path().join("diagram folder");
    std::fs::create_dir(&folder).unwrap();
    let shown = draw(
        &workspace,
        &plantuml,
        &folder,
        "flow.puml",
        "@startuml
!include common.puml
@enduml
",
        cx,
    );
    let svg = shown.read_with(cx, |preview, _| preview.svgs()).remove(0);
    let svg = String::from_utf8(svg).unwrap();
    let tmp = dir.path().file_name().unwrap().to_str().unwrap();
    assert!(
        svg.contains(tmp) && svg.ends_with("diagram folder</svg>"),
        "{svg}"
    );
}

/// `folders` as an include path, as PlantUML reads it.
fn joined(folders: &[&Path]) -> std::ffi::OsString {
    std::env::join_paths(folders).unwrap()
}

#[gpui_kit::test]
fn plantuml_gets_the_documents_folder_as_its_include_path(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    // The diagram is the include path PlantUML was given.
    let plantuml = fake_plantuml(
        dir.path(),
        "echo ^<svg^>%PLANTUML_INCLUDE_PATH%^</svg^>",
        "echo \"<svg>$PLANTUML_INCLUDE_PATH</svg>\"",
    );
    let folder = dir.path().join("views");
    std::fs::create_dir(&folder).unwrap();
    let library = dir.path().join("library");
    let setting = library.display().to_string();
    cx.update(|_, cx| {
        cx.global_mut::<AppState>()
            .settings
            .preview
            .plantuml_include_path = setting;
    });
    let shown = draw(
        &workspace,
        &plantuml,
        &folder,
        "context.puml",
        "@startuml
!include ../shared/context.iuml
@enduml
",
        cx,
    );
    let svg = shown.read_with(cx, |preview, _| preview.svgs()).remove(0);
    let svg = String::from_utf8(svg).unwrap();
    let expected = joined(&[&folder, &library]);
    let expected = expected.to_str().unwrap();
    assert!(
        svg.starts_with(&format!("<svg>{expected}")),
        "{svg} starts with {expected}"
    );
}

#[gpui_kit::test]
fn ctrl_click_on_an_include_opens_its_file(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let document = dir.path().join("flow.puml");
    std::fs::write(&document, "@startuml\n!include common.iuml\n@enduml\n").unwrap();
    std::fs::write(dir.path().join("common.iuml"), "class Common\n").unwrap();
    let invocation = |target: &str| {
        birchpad_commands::Invocation::with_args(
            "preview.open-include",
            serde_json::json!({ "target": target, "document": document }),
        )
    };
    let opened = workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&invocation("common.iuml"), window, cx)
    });
    cx.run_until_parked();
    opened.unwrap();
    let active = workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.read(cx).buffer.read(cx).path().map(Path::to_path_buf)
    });
    assert_eq!(active, Some(dir.path().join("common.iuml")));
    // A file that is not there is said, not opened.
    let missing = workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&invocation("gone.iuml"), window, cx)
    });
    assert!(missing.unwrap_err().to_string().contains("gone.iuml"));
}

/// A stand-in for PlantUML whose diagram is `common.iuml`, next to the document.
fn plantuml_showing_common(dir: &Path) -> PathBuf {
    fake_plantuml(
        dir,
        "echo ^<svg^>\ntype common.iuml\necho ^</svg^>",
        "echo \"<svg>$(cat common.iuml)</svg>\"",
    )
}

fn diagram_text(shown: &Entity<Preview>, cx: &mut VisualTestContext) -> String {
    String::from_utf8(shown.read_with(cx, |preview, _| preview.svgs()).remove(0)).unwrap()
}

/// Waits until the diagram shows `text`.
fn wait_for_diagram(shown: &Entity<Preview>, text: &str, cx: &mut VisualTestContext) {
    for _ in 0..1000 {
        cx.run_until_parked();
        if diagram_text(shown, cx).contains(text) {
            return;
        }
        cx.executor().advance_clock(Duration::from_millis(200));
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!(
        "the diagram never showed {text}: {}",
        diagram_text(shown, cx)
    );
}

#[gpui_kit::test]
fn redraw_draws_what_is_on_disk_now(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let plantuml = plantuml_showing_common(dir.path());
    std::fs::write(dir.path().join("common.iuml"), "class First").unwrap();
    let shown = draw(
        &workspace,
        &plantuml,
        dir.path(),
        "flow.puml",
        "@startuml\n!include common.iuml\n@enduml\n",
        cx,
    );
    assert!(diagram_text(&shown, cx).contains("class First"));
    assert_eq!(
        shown.read_with(cx, |preview, _| preview.included()),
        [dir.path().join("common.iuml")]
    );
    assert!(
        shown
            .read_with(cx, |preview, _| preview.watched_folders())
            .contains(&dir.path().to_path_buf())
    );
    std::fs::write(dir.path().join("common.iuml"), "class Second").unwrap();
    run(&workspace, "preview.redraw", cx).unwrap();
    wait_for_diagram(&shown, "class Second", cx);
}

#[gpui_kit::test]
fn a_change_to_an_included_file_redraws(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let plantuml = plantuml_showing_common(dir.path());
    std::fs::write(dir.path().join("common.iuml"), "class First").unwrap();
    let shown = draw(
        &workspace,
        &plantuml,
        dir.path(),
        "flow.puml",
        "@startuml\n!include common.iuml\n@enduml\n",
        cx,
    );
    // Changed by another program: no command, the watcher tells the preview (in tests, the
    // test tells it, as the system's watcher's thread would upset the test scheduler).
    std::fs::write(dir.path().join("common.iuml"), "class Changed").unwrap();
    let common = dir.path().join("common.iuml");
    shown.update(cx, |preview, cx| preview.files_changed(&[common], cx));
    wait_for_diagram(&shown, "class Changed", cx);
    // A file the diagram does not include, and no PlantUML file: nothing drawn again.
    std::fs::write(dir.path().join("common.iuml"), "class Unseen").unwrap();
    let notes = dir.path().join("notes.txt");
    shown.update(cx, |preview, cx| preview.files_changed(&[notes], cx));
    cx.run_until_parked();
    assert!(diagram_text(&shown, cx).contains("class Changed"));
}

#[gpui_kit::test]
fn redraw_needs_plantuml_diagrams(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    assert!(run(&workspace, "preview.redraw", cx).is_err());
}

fn run(workspace: &Entity<Workspace>, id: &str, cx: &mut VisualTestContext) -> anyhow::Result<()> {
    let result = workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&birchpad_commands::Invocation::new(id), window, cx)
    });
    cx.run_until_parked();
    result
}

#[gpui_kit::test]
fn the_diagram_background_switches_and_is_saved(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    cx.update(|_, cx| cx.global_mut::<AppState>().paths.user_settings = Some(path.clone()));
    assert_eq!(
        cx.update(|_, cx| background(cx)),
        DiagramBackground::White,
        "white by default"
    );
    run(&workspace, "preview.white-background", cx).unwrap();
    assert_eq!(cx.update(|_, cx| background(cx)), DiagramBackground::Theme);
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(
        written.contains("plantuml-background = \"theme\""),
        "{written}"
    );
    run(&workspace, "preview.white-background", cx).unwrap();
    assert_eq!(cx.update(|_, cx| background(cx)), DiagramBackground::White);
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("plantuml-background = \"white\"")
    );
}

#[gpui_kit::test]
fn the_diagram_background_is_not_switched_without_a_settings_file(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.update(|_, cx| cx.global_mut::<AppState>().paths.user_settings = None);
    assert!(run(&workspace, "preview.white-background", cx).is_err());
    assert_eq!(cx.update(|_, cx| background(cx)), DiagramBackground::White);
}

/// The card of the first diagram, as laid out: its width and height.
fn card(cx: &mut VisualTestContext) -> (f32, f32) {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let bounds = cx.debug_bounds("plantuml-diagram-0").expect("a card");
    (f32::from(bounds.size.width), f32::from(bounds.size.height))
}

/// Roadmap 7.4x: the first drawing at Fit had a card as tall as the preview, the diagram at its
/// top, until the zoom changed; its size came from its image's, unknown before it is decoded.
#[gpui_kit::test]
fn the_first_drawing_at_fit_is_as_tall_as_its_diagram(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let plantuml = sized_plantuml(dir.path(), 300, 120);
    let shown = draw(
        &workspace,
        &plantuml,
        dir.path(),
        "small.puml",
        "@startuml
A -> B
@enduml
",
        cx,
    );
    assert_eq!(shown.read_with(cx, |preview, _| preview.zoom()), Zoom::Fit);
    assert_eq!(card(cx), (300., 120.), "at its size, no taller");
    // A wide one: as wide as the room, its height in proportion.
    let plantuml = sized_plantuml(dir.path(), 6000, 2000);
    draw(
        &workspace,
        &plantuml,
        dir.path(),
        "wide.puml",
        "@startuml
A -> B
@enduml
",
        cx,
    );
    let (width, height) = card(cx);
    assert!(width < 6000., "{width}");
    assert!((height - width / 3.).abs() < 1., "{width} x {height}");
}

fn simple_plantuml(dir: &Path) -> PathBuf {
    fake_plantuml(
        dir,
        "echo ^<svg width=\"200px\" height=\"100px\" viewBox=\"0 0 200 100\"^>^</svg^>",
        "echo '<svg width=\"200px\" height=\"100px\" viewBox=\"0 0 200 100\"></svg>'",
    )
}

#[gpui_kit::test]
fn the_zoom_commands_zoom_the_active_diagrams(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let plantuml = simple_plantuml(dir.path());
    let shown = draw(
        &workspace,
        &plantuml,
        dir.path(),
        "flow.puml",
        "@startuml\nA -> B\n@enduml\n",
        cx,
    );
    let zoom = |cx: &mut VisualTestContext| shown.read_with(cx, |preview, _| preview.zoom());
    assert_eq!(zoom(cx), Zoom::Fit);
    run(&workspace, "preview.zoom-in", cx).unwrap();
    assert_eq!(zoom(cx), Zoom::Percent(110));
    run(&workspace, "preview.zoom-in", cx).unwrap();
    run(&workspace, "preview.zoom-out", cx).unwrap();
    assert_eq!(zoom(cx), Zoom::Percent(110));
    run(&workspace, "preview.actual-size", cx).unwrap();
    assert_eq!(zoom(cx), Zoom::Percent(100));
    for _ in 0..20 {
        run(&workspace, "preview.zoom-in", cx).unwrap();
    }
    assert_eq!(zoom(cx), Zoom::Percent(400));
    run(&workspace, "preview.fit", cx).unwrap();
    assert_eq!(zoom(cx), Zoom::Fit);
    run(&workspace, "preview.fit-page", cx).unwrap();
    assert_eq!(zoom(cx), Zoom::FitPage);
    run(&workspace, "preview.zoom-in", cx).unwrap();
    assert_eq!(zoom(cx), Zoom::Percent(110), "from 100%, as from Fit");
}

/// A drag over the diagrams with the left button, from their center by `by`.
fn drag(by: Point<Pixels>, cx: &mut VisualTestContext) {
    let from = cx.debug_bounds("plantuml-diagrams").unwrap().center();
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(from + by, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(from + by, MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
}

#[gpui_kit::test]
fn dragging_a_diagram_scrolls_it(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let plantuml = sized_plantuml(dir.path(), 3000, 2000);
    let shown = draw(
        &workspace,
        &plantuml,
        dir.path(),
        "wide.puml",
        "@startuml\nA -> B\n@enduml\n",
        cx,
    );
    run(&workspace, "preview.actual-size", cx).unwrap();
    let (_, most) = scrolled(&shown, cx);
    drag(point(px(-120.), px(-80.)), cx);
    assert_eq!(scrolled(&shown, cx).0, point(px(-120.), px(-80.)));
    assert!(
        !shown.read_with(cx, |preview, _| preview.dragging()),
        "let go"
    );
    // Back past the start: no further than the edge.
    drag(point(px(500.), px(500.)), cx);
    assert_eq!(scrolled(&shown, cx).0, point(px(0.), px(0.)));
    drag(point(px(-1_000_000.), px(0.)), cx);
    assert_eq!(scrolled(&shown, cx).0.x, -most.x);
}

#[gpui_kit::test]
fn fit_page_shows_a_tall_diagram_whole(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let plantuml = sized_plantuml(dir.path(), 400, 20_000);
    let shown = draw(
        &workspace,
        &plantuml,
        dir.path(),
        "tall.puml",
        "@startuml\nA -> B\n@enduml\n",
        cx,
    );
    // At Fit, as wide as it is: far taller than the preview.
    assert!(scrolled(&shown, cx).1.y > px(0.));
    run(&workspace, "preview.fit-page", cx).unwrap();
    // Laid out once to know the room, then shown whole.
    scrolled(&shown, cx);
    let (_, most) = scrolled(&shown, cx);
    assert_eq!(most, point(px(0.), px(0.)), "nothing to scroll");
}

/// A PlantUML that draws one diagram `width` by `height` pixels.
fn sized_plantuml(dir: &Path, width: u32, height: u32) -> PathBuf {
    let svg =
        format!("svg width=\"{width}px\" height=\"{height}px\" viewBox=\"0 0 {width} {height}\"");
    fake_plantuml(
        dir,
        &format!("echo ^<{svg}^>^</svg^>"),
        &format!("echo '<{svg}></svg>'"),
    )
}

/// The diagrams laid out again: how far they are scrolled and how far they can be.
fn scrolled(shown: &Entity<Preview>, cx: &mut VisualTestContext) -> (Point<Pixels>, Point<Pixels>) {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    shown.read_with(cx, |preview, _| preview.scroll().unwrap())
}

/// The mouse wheel over the diagrams, by `delta` pixels, with `modifiers` held.
fn wheel(delta: Point<Pixels>, modifiers: Modifiers, cx: &mut VisualTestContext) {
    let area = cx.debug_bounds("plantuml-diagrams").unwrap();
    cx.simulate_event(ScrollEvent {
        position: area.center(),
        delta: ScrollDelta::Pixels(delta),
        modifiers,
        ..Default::default()
    });
    cx.run_until_parked();
}

/// #58: a zoomed diagram wider than the preview scrolls across too, with Shift and the wheel
/// as with a wheel or a touchpad that scrolls across, and no further than its edges.
#[gpui_kit::test]
fn a_zoomed_diagram_scrolls_both_ways(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let plantuml = sized_plantuml(dir.path(), 3000, 2000);
    let shown = draw(
        &workspace,
        &plantuml,
        dir.path(),
        "wide.puml",
        "@startuml
A -> B
@enduml
",
        cx,
    );
    // At Fit, the diagram is as wide as the preview.
    let (_, most) = scrolled(&shown, cx);
    assert_eq!(most.x, px(0.), "nothing to scroll across at Fit");
    run(&workspace, "preview.actual-size", cx).unwrap();
    let (offset, most) = scrolled(&shown, cx);
    assert_eq!(offset, point(px(0.), px(0.)));
    assert!(most.x > px(0.) && most.y > px(0.), "{most:?}");

    // Shift with the wheel scrolls across, not down.
    let shift = Modifiers::shift();
    wheel(point(px(0.), px(-100.)), shift, cx);
    assert_eq!(scrolled(&shown, cx).0, point(px(-100.), px(0.)));
    // A wheel that scrolls across does it without Shift.
    wheel(point(px(-50.), px(0.)), Modifiers::default(), cx);
    assert_eq!(scrolled(&shown, cx).0, point(px(-150.), px(0.)));
    // The wheel alone scrolls down.
    wheel(point(px(0.), px(-40.)), Modifiers::default(), cx);
    assert_eq!(scrolled(&shown, cx).0, point(px(-150.), px(-40.)));

    // No further than the right edge, nor back past the left one.
    wheel(point(px(0.), px(-1_000_000.)), shift, cx);
    assert_eq!(scrolled(&shown, cx).0.x, -most.x);
    wheel(point(px(0.), px(1_000_000.)), shift, cx);
    assert_eq!(scrolled(&shown, cx).0.x, px(0.));

    // Ctrl with the wheel zooms; it does not scroll.
    wheel(point(px(0.), px(-100.)), shift, cx);
    wheel(
        point(px(0.), px(120.)),
        cx.update(|_, cx| Modifiers::secondary_key(cx)),
        cx,
    );
    assert_eq!(
        shown.read_with(cx, |preview, _| preview.zoom()),
        Zoom::Percent(110)
    );
    assert_eq!(scrolled(&shown, cx).0.x, px(-100.));
}

/// A diagram that fits has nothing to scroll across: Shift with the wheel leaves it.
#[gpui_kit::test]
fn a_diagram_that_fits_does_not_scroll_across(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let plantuml = sized_plantuml(dir.path(), 200, 100);
    let shown = draw(
        &workspace,
        &plantuml,
        dir.path(),
        "small.puml",
        "@startuml
A -> B
@enduml
",
        cx,
    );
    run(&workspace, "preview.actual-size", cx).unwrap();
    let (offset, most) = scrolled(&shown, cx);
    assert_eq!(most.x, px(0.));
    wheel(point(px(0.), px(-100.)), Modifiers::shift(), cx);
    assert_eq!(scrolled(&shown, cx).0, offset);
}

#[gpui_kit::test]
fn the_zoom_commands_need_plantuml_diagrams(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.md");
    std::fs::write(&path, "# Notes\n").unwrap();
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(&path, window, cx);
    });
    cx.run_until_parked();
    for (id, _) in ZOOM_COMMANDS {
        let error = run(&workspace, id, cx).unwrap_err().to_string();
        assert!(error.contains("PlantUML"), "{id}: {error}");
    }
}

/// The width and height a PNG file says it has.
fn png_size(png: &[u8]) -> (u32, u32) {
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let number = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().unwrap());
    (number(16), number(20))
}

#[gpui_kit::test]
fn diagrams_are_exported_as_svg_and_png(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let plantuml = simple_plantuml(dir.path());
    let shown = draw(
        &workspace,
        &plantuml,
        dir.path(),
        "flow.puml",
        "@startuml\nA -> B\n@enduml\n",
        cx,
    );
    let drawn = shown
        .read_with(cx, |preview, _| preview.diagram_svg(0))
        .unwrap();
    let export = |id: &str, path: &Path, cx: &mut VisualTestContext| {
        let invocation = birchpad_commands::Invocation::with_args(
            id,
            serde_json::json!({ "diagram": 0, "path": path }),
        );
        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.dispatch(&invocation, window, cx)
            })
            .unwrap();
        for _ in 0..20 {
            cx.run_until_parked();
        }
    };
    let svg = dir.path().join("out.svg");
    export("preview.export-svg", &svg, cx);
    assert_eq!(std::fs::read(&svg).unwrap(), drawn, "PlantUML's own SVG");
    let png = dir.path().join("out.png");
    export("preview.export-png", &png, cx);
    assert_eq!(png_size(&std::fs::read(&png).unwrap()), (400, 200), "twice");
    cx.update(|_, cx| {
        cx.global_mut::<AppState>()
            .settings
            .preview
            .plantuml_png_scale = 1;
    });
    export("preview.export-png", &png, cx);
    assert_eq!(png_size(&std::fs::read(&png).unwrap()), (200, 100));
    // A scale out of its range is brought into it.
    cx.update(|_, cx| {
        cx.global_mut::<AppState>()
            .settings
            .preview
            .plantuml_png_scale = 0;
    });
    export("preview.export-png", &png, cx);
    assert_eq!(png_size(&std::fs::read(&png).unwrap()), (200, 100));
}

#[gpui_kit::test]
fn a_diagram_is_copied_as_a_picture(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let plantuml = simple_plantuml(dir.path());
    draw(
        &workspace,
        &plantuml,
        dir.path(),
        "flow.puml",
        "@startuml\nA -> B\n@enduml\n",
        cx,
    );
    run(&workspace, "preview.copy-diagram", cx).unwrap();
    for _ in 0..20 {
        cx.run_until_parked();
    }
    let copied = cx.update(|_, cx| cx.read_from_clipboard()).unwrap();
    let image = copied
        .entries()
        .iter()
        .find_map(|entry| match entry {
            gpui_kit::ClipboardEntry::Image(image) => Some(image.clone()),
            _ => None,
        })
        .expect("an image");
    assert_eq!(image.format(), gpui_kit::ImageFormat::Png);
    assert_eq!(png_size(image.bytes()), (400, 200));
}

#[gpui_kit::test]
fn only_diagrams_that_were_drawn_are_exported(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let plantuml = simple_plantuml(dir.path());
    draw(
        &workspace,
        &plantuml,
        dir.path(),
        "flow.puml",
        "@startuml\nA -> B\n@enduml\n",
        cx,
    );
    let path = dir.path().join("second.svg");
    let invocation = birchpad_commands::Invocation::with_args(
        "preview.export-svg",
        serde_json::json!({ "diagram": 1, "path": path }),
    );
    let refused = workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&invocation, window, cx)
    });
    assert!(refused.is_err(), "there is one diagram");
    assert!(!path.exists());
    // A Markdown document has none.
    let notes = dir.path().join("notes.md");
    std::fs::write(&notes, "# Notes\n").unwrap();
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(&notes, window, cx);
    });
    cx.run_until_parked();
    assert!(run(&workspace, "preview.copy-diagram", cx).is_err());
}

#[gpui_kit::test]
fn the_picture_of_a_placed_mistake_shows_on_a_click(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    let plantuml = fake_plantuml(
        dir.path(),
        "echo ^<svg width=\"200px\" height=\"80px\"^>^<text^>Syntax Error?^</text^>^</svg^>\n\
         >&2 echo ERROR\n>&2 echo 1\n>&2 echo Syntax Error?\nexit /b 200",
        "echo '<svg width=\"200px\" height=\"80px\"><text>Syntax Error?</text></svg>'\n\
         echo ERROR >&2\necho 1 >&2\necho 'Syntax Error?' >&2\nexit 200",
    );
    let shown = draw(
        &workspace,
        &plantuml,
        dir.path(),
        "flow.puml",
        "@startuml\nqux\n@enduml\n",
        cx,
    );
    assert_eq!(
        shown.read_with(cx, |preview, _| preview.folded()),
        (vec![0], vec![])
    );
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("plantuml-diagram-0").is_none(), "folded");
    shown.update(cx, |preview, cx| preview.toggle_picture(0, cx));
    assert_eq!(
        shown.read_with(cx, |preview, _| preview.folded()),
        (vec![0], vec![0])
    );
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("plantuml-diagram-0").is_some(), "shown");
    // Folded again; a diagram that is no such picture is not toggled.
    shown.update(cx, |preview, cx| {
        preview.toggle_picture(0, cx);
        preview.toggle_picture(5, cx);
    });
    assert_eq!(
        shown.read_with(cx, |preview, _| preview.folded()),
        (vec![0], vec![])
    );
}

/// A PlantUML that crashes with smetana, as on the diagrams of 7.4ab, and draws with ELK; with
/// `elk_crashes`, ELK crashes too. (Rust quotes `-Playout=smetana` for cmd, which splits it in
/// two at `=` otherwise: both forms are looked at.)
fn plantuml_smetana_crashes(dir: &Path, elk_crashes: bool) -> PathBuf {
    let elk = if elk_crashes {
        "smetana elk"
    } else {
        "smetana"
    };
    let windows_elk = if elk_crashes {
        "if \"%~7\"==\"-Playout=elk\" goto crash\nif \"%8\"==\"elk\" goto crash\n"
    } else {
        ""
    };
    fake_plantuml(
        dir,
        &format!(
            "if \"%~7\"==\"-Playout=smetana\" goto crash\n\
             if \"%8\"==\"smetana\" goto crash\n{windows_elk}\
             echo ^<svg^>drawn with %~7 %8^</svg^>\nexit /b 0\n:crash\n\
             echo ^<svg^>crash %~7 %8^</svg^>\n\
             >&2 echo java.lang.NullPointerException: splines is null\n\
             >&2 echo \tat a.SmetanaEdge.draw(SmetanaEdge.java:1)\nexit /b 1"
        ),
        &format!(
            "for layout in {elk}; do\n\
             if [ \"$7\" = \"-Playout=$layout\" ]; then\n\
             echo \"<svg>crash $7</svg>\"\n\
             echo 'java.lang.NullPointerException: splines is null' >&2\n\
             printf '\\tat a.SmetanaEdge.draw(SmetanaEdge.java:1)\\n' >&2\n\
             exit 1\nfi\ndone\n\
             echo \"<svg>drawn with $7</svg>\""
        ),
    )
}

#[gpui_kit::test]
fn the_preview_draws_with_elk_where_smetana_crashes(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    DOT.with(|dot| *dot.borrow_mut() = Some(None));
    let dir = tempfile::tempdir().unwrap();
    let plantuml = plantuml_smetana_crashes(dir.path(), false);
    let shown = draw(
        &workspace,
        &plantuml,
        dir.path(),
        "large.puml",
        "@startuml\nA -> B\n@enduml\n",
        cx,
    );
    let svg =
        String::from_utf8(shown.read_with(cx, |preview, _| preview.svgs()).remove(0)).unwrap();
    assert!(svg.contains("drawn with") && svg.contains("elk"), "{svg}");
    assert!(shown.read_with(cx, |preview, _| preview.fell_back_to_elk()));
    assert_eq!(shown.read_with(cx, |preview, _| preview.layout()), "ELK");
    assert!(
        shown
            .read_with(cx, |preview, _| preview.mistakes())
            .is_empty()
    );
    assert!(!shown.read_with(cx, |preview, _| preview.waiting_for_graphviz()));
    DOT.with(|dot| *dot.borrow_mut() = None);
}

#[gpui_kit::test]
fn a_crash_of_elk_too_shows_the_crash_of_smetana(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    DOT.with(|dot| *dot.borrow_mut() = Some(None));
    let dir = tempfile::tempdir().unwrap();
    let plantuml = plantuml_smetana_crashes(dir.path(), true);
    let shown = draw(
        &workspace,
        &plantuml,
        dir.path(),
        "large.puml",
        "@startuml\nA -> B\n@enduml\n",
        cx,
    );
    assert!(!shown.read_with(cx, |preview, _| preview.fell_back_to_elk()));
    assert_eq!(
        shown.read_with(cx, |preview, _| preview.elk_attempt()),
        ElkAttempt::Crashed,
        "said: neither smetana nor ELK"
    );
    assert_eq!(
        shown.read_with(cx, |preview, _| preview.layout()),
        "smetana"
    );
    let found = shown.read_with(cx, |preview, _| preview.mistakes());
    assert!(found[0].details.is_some(), "the crash, with its details");
    DOT.with(|dot| *dot.borrow_mut() = None);
}

#[gpui_kit::test]
fn the_picture_of_a_crash_is_folded_with_its_details(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    DOT.with(|dot| *dot.borrow_mut() = Some(None));
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().settings.preview.plantuml_layout =
            birchpad_config::DiagramLayout::Smetana;
    });
    let dir = tempfile::tempdir().unwrap();
    let plantuml = fake_plantuml(
        dir.path(),
        "echo ^<svg width=\"200px\" height=\"80px\"^>java.lang.NullPointerException^</svg^>\n\
         >&2 echo java.lang.NullPointerException\n\
         >&2 echo 	at a.SmetanaEdge.draw(SmetanaEdge.java:1)\nexit /b 1",
        "echo '<svg width=\"200px\" height=\"80px\">java.lang.NullPointerException</svg>'\n\
         echo java.lang.NullPointerException >&2\n\
         printf '\tat a.SmetanaEdge.draw(SmetanaEdge.java:1)\n' >&2\nexit 1",
    );
    let shown = draw(
        &workspace,
        &plantuml,
        dir.path(),
        "large.puml",
        "@startuml\nA -> B\n@enduml\n",
        cx,
    );
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("plantuml-diagram-0").is_none(), "folded");
    shown.update(cx, |preview, cx| preview.toggle_details(cx));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("plantuml-diagram-0").is_some(),
        "shown with the details"
    );
    DOT.with(|dot| *dot.borrow_mut() = None);
}

#[test]
fn the_plantuml_language_has_the_diagrams_preview() {
    assert_eq!(
        kind_for(Some("plantuml"), None),
        Some(PreviewKind::PlantUml)
    );
    assert_eq!(
        kind_for(Some("plantuml"), Some(Path::new("notes.txt"))),
        Some(PreviewKind::PlantUml)
    );
    assert_eq!(kind_for(Some("rust"), Some(Path::new("a.rs"))), None);
}
#[test]
fn preferences_show_the_preview_settings() {
    use birchpad::preferences::{ENTRIES, Kind};
    let find = |key: &str| ENTRIES.iter().find(|entry| entry.key == key).unwrap();
    assert_eq!(find("preview.plantuml").kind, Kind::Text);
    assert_eq!(find("preview.plantuml-include-path").kind, Kind::Text);
    let Kind::Choice(layouts) = find("preview.plantuml-layout").kind else {
        panic!("a choice");
    };
    for (value, _) in layouts {
        birchpad_config::check_setting(
            "preview.plantuml-layout",
            &toml::Value::String((*value).into()),
        )
        .unwrap();
    }
    assert_eq!(layouts.len(), 4);
    let Kind::Choice(values) = find("preview.plantuml-background").kind else {
        panic!("a choice");
    };
    for (value, _) in values {
        birchpad_config::check_setting(
            "preview.plantuml-background",
            &toml::Value::String((*value).into()),
        )
        .unwrap();
    }
    assert_eq!(values.len(), 2);
    assert!(
        ENTRIES
            .iter()
            .filter(|entry| entry.key.starts_with("preview."))
            .all(|entry| entry.section == "Preview")
    );
}
