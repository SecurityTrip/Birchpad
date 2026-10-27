//! Tests of the PlantUML preview (ADR 0057): its runs, stopped when the text moves on, and the
//! mistakes PlantUML finds, placed on the lines of the document.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use crate::workspace::open_workspace;
use birchpad::app_state::AppState;
use birchpad::preview::*;
use birchpad::workspace::Workspace;
use birchpad_config::DiagramBackground;

#[test]
fn diagrams_start_at_their_start_lines() {
    let text = "' a comment\n@startuml\nA -> B\n@enduml\n\n  @startmindmap\n* root\n@endmindmap";
    assert_eq!(diagram_starts(text), [1, 5]);
    assert_eq!(diagram_starts("@STARTUML\n@enduml"), [0], "in any case");
    assert!(diagram_starts("").is_empty());
    assert!(
        diagram_starts("@enduml\n' @startuml\n@star").is_empty(),
        "ends, comments and a cut word start nothing"
    );
    assert_eq!(diagram_starts("\n\n@startuml"), [2], "on the last line");
}

/// PlantUML's picture of a mistake, with its message.
fn picture(message: &str) -> Vec<u8> {
    format!("<svg><text>{message}</text></svg>").into_bytes()
}

#[test]
fn mistakes_are_placed_on_the_lines_of_their_diagrams() {
    let text = "\n\n@startuml\nA -> B\n@enduml\n\n@startuml\nA -> B\nfoo bar baz\n@enduml\n";
    let error = "Syntax Error? (Assumed diagram type: sequence)";
    let diagrams = [b"<svg>A B</svg>".to_vec(), picture(error)];
    let stderr = format!("ERROR\r\n2\r\n{error}\r\n");
    assert_eq!(
        mistakes(&stderr, text, &diagrams),
        [Mistake {
            line: Some(8),
            message: error.to_owned()
        }],
        "the second diagram's third line"
    );
    // Both diagrams with mistakes, the second one's message with `<`, as SVG writes it.
    let text = "@startuml\nqux\n@enduml\n@startuml\nA -> B\nfoo <baz>\n@enduml\n";
    let diagrams = [picture("Syntax Error?"), picture("cannot include &lt;baz>")];
    let stderr = "ERROR\n1\nSyntax Error?\nERROR\n2\ncannot include <baz>\n";
    let found = mistakes(stderr, text, &diagrams);
    assert_eq!(
        found.iter().map(|mistake| mistake.line).collect::<Vec<_>>(),
        [Some(1), Some(5)]
    );
    assert_eq!(found[1].message, "cannot include <baz>");
}

#[test]
fn other_error_output_is_one_mistake_without_a_line() {
    let diagrams = [picture("?")];
    assert_eq!(
        mistakes(
            "Exception in thread main\n  at Foo\n",
            "@startuml\n@enduml",
            &diagrams
        ),
        [Mistake {
            line: None,
            message: "Exception in thread main\nat Foo".to_owned()
        }]
    );
    assert!(mistakes("", "@startuml\n@enduml", &diagrams).is_empty());
    assert!(mistakes(" \n\n", "@startuml\n@enduml", &diagrams).is_empty());
    // Cut short: what there is is shown as it is.
    assert_eq!(mistakes("ERROR\n2", "@startuml", &diagrams)[0].line, None);
    // A mistake in a text without `@start` lines has no line in it.
    assert_eq!(
        mistakes("ERROR\n0\nNo diagram", "A -> B", &[])[0].line,
        None
    );
}

#[test]
fn lines_start_after_any_line_end() {
    let text = "a\nbb\r\nccc\rd";
    assert_eq!(line_offset(text, 0), 0);
    assert_eq!(line_offset(text, 1), 2);
    assert_eq!(line_offset(text, 2), 6);
    assert_eq!(line_offset(text, 3), 10);
    assert_eq!(line_offset(text, 4), text.len(), "past the last line");
    assert_eq!(line_offset("", 0), 0);
    assert_eq!(line_offset("", 3), 0);
    assert_eq!(
        line_offset("a\n", 1),
        2,
        "the empty line after the last end"
    );
}

/// A program that runs for half a minute unless it is stopped.
fn long_running() -> (String, Vec<String>) {
    if cfg!(windows) {
        (
            "ping".into(),
            ["-n", "30", "127.0.0.1"].map(String::from).to_vec(),
        )
    } else {
        ("sleep".into(), vec!["30".into()])
    }
}

#[test]
fn a_stopped_run_ends_its_process() {
    let (program, args) = long_running();
    let stop = Stop::default();
    let running = stop.clone();
    let started = Instant::now();
    let run =
        std::thread::spawn(move || run_plantuml(&program, &args, None, "@startuml", &running));
    std::thread::sleep(Duration::from_millis(300));
    drop(StopOnDrop(stop));
    assert_eq!(run.join().unwrap(), Err(NotDrawn::Stopped));
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "the process was ended, not waited for"
    );
}

#[test]
fn a_run_stopped_before_it_starts_runs_nothing() {
    let stop = Stop::default();
    stop.stop();
    // Started, it would say that this program is missing.
    assert_eq!(
        run_plantuml("birchpad-test-no-such-plantuml", &[], None, "", &stop),
        Err(NotDrawn::Stopped)
    );
    assert!(matches!(
        run_plantuml(
            "birchpad-test-no-such-plantuml",
            &[],
            None,
            "",
            &Stop::default()
        ),
        Err(NotDrawn::Missing(_))
    ));
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
            message: "Syntax Error?".into()
        }]
    );
    shown.update_in(cx, |preview, window, cx| preview.go_to_line(2, window, cx));
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

#[test]
fn relative_settings_are_taken_from_birchpads_folder() {
    let base = if cfg!(windows) {
        PathBuf::from(r"C:irchpad")
    } else {
        PathBuf::from("/birchpad")
    };
    let joined = |path: &str| base.join(path).display().to_string();
    assert_eq!(
        setting_path("tools/plantuml.jar", &base),
        joined("tools/plantuml.jar")
    );
    assert_eq!(
        setting_path(" plantuml.jar ", &base),
        joined("plantuml.jar")
    );
    assert_eq!(setting_path("bin/plantuml", &base), joined("bin/plantuml"));
    // A name looked for on `PATH`, an absolute path, the default.
    assert_eq!(setting_path("plantuml", &base), "plantuml");
    assert_eq!(setting_path("my-plantuml.cmd", &base), "my-plantuml.cmd");
    let absolute = base.join("opt").join("plantuml.jar").display().to_string();
    assert_eq!(setting_path(&absolute, &base), absolute);
    assert_eq!(setting_path("  ", &base), "");
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

#[test]
fn preferences_show_the_preview_settings() {
    use birchpad::preferences::{ENTRIES, Kind};
    let find = |key: &str| ENTRIES.iter().find(|entry| entry.key == key).unwrap();
    assert_eq!(find("preview.plantuml").kind, Kind::Text);
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

#[test]
fn zoom_goes_by_steps_and_stops_at_the_ends() {
    assert_eq!(Zoom::default(), Zoom::Fit);
    assert_eq!(Zoom::Fit.zoom_in(), Zoom::Percent(110), "from 100%");
    assert_eq!(Zoom::Fit.zoom_out(), Zoom::Percent(90));
    assert_eq!(Zoom::Percent(100).zoom_in(), Zoom::Percent(110));
    assert_eq!(Zoom::Percent(150).zoom_out(), Zoom::Percent(125));
    assert_eq!(Zoom::Percent(400).zoom_in(), Zoom::Percent(400));
    assert_eq!(Zoom::Percent(25).zoom_out(), Zoom::Percent(25));
    // Between steps: the next one either way.
    assert_eq!(Zoom::Percent(105).zoom_in(), Zoom::Percent(110));
    assert_eq!(Zoom::Percent(105).zoom_out(), Zoom::Percent(100));
    assert_eq!(Zoom::Percent(1000).zoom_in(), Zoom::Percent(400));
    assert_eq!(Zoom::Percent(125).label(), "125%");
    assert_eq!(Zoom::Percent(125).percent(), Some(125));
    assert_eq!(Zoom::Fit.percent(), None);
    assert!(ZOOM_STEPS.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn an_svg_has_its_size_from_its_attributes_or_its_view_box() {
    let plantuml = br#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" style="width:533px;height:367px;background:#FFFFFF;" width="533px" height="367px" viewBox="0 0 533 367"><g/></svg>"#;
    assert_eq!(svg_size(plantuml), Some((533., 367.)));
    assert_eq!(
        svg_size(br#"<svg width="10.5" height="20"></svg>"#),
        Some((10.5, 20.))
    );
    assert_eq!(
        svg_size(br#"<svg width="100%" height="100%" viewBox="0,0 40 30"></svg>"#),
        Some((40., 30.)),
        "other units: the view box"
    );
    assert_eq!(svg_size(br#"<svg viewBox="0 0 0 30"></svg>"#), None);
    assert_eq!(svg_size(br#"<svg width="0" height="0"></svg>"#), None);
    assert_eq!(svg_size(b"<svg></svg>"), None);
    assert_eq!(svg_size(b"no picture"), None);
    assert_eq!(svg_size(&[0xff, 0xfe]), None);
}

#[test]
fn a_sized_svg_keeps_its_view_box() {
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" style="width:533px;height:367px;background:#FFFFFF;" width="533px" height="367px" viewBox="0 0 533 367"><rect width="5" height="5"/></svg>"#;
    let sized = String::from_utf8(sized_svg(svg, (1066., 734.))).unwrap();
    assert!(
        sized.contains(r#" width="1066px" height="734px""#),
        "{sized}"
    );
    assert!(
        sized.contains("style=\"width:1066px;height:734px;background:#FFFFFF;\""),
        "{sized}"
    );
    assert!(sized.contains(r#"viewBox="0 0 533 367""#));
    assert!(
        sized.contains(r#"<rect width="5" height="5"/>"#),
        "only the root changes"
    );
    assert_eq!(svg_size(sized.as_bytes()), Some((1066., 734.)));
    // Without a size: given one.
    let sized = sized_svg(br#"<svg viewBox="0 0 4 3"></svg>"#, (8., 6.));
    assert_eq!(svg_size(&sized), Some((8., 6.)));
    // Not an SVG: left as it is.
    assert_eq!(sized_svg(b"text", (1., 1.)), b"text");
    assert_eq!(sized_svg(&[0xff], (1., 1.)), [0xff]);
}

#[test]
fn zoomed_diagrams_are_drawn_larger_within_a_limit() {
    assert_eq!(raster_factor((500., 400.), 2.), 2.);
    assert_eq!(
        raster_factor((500., 400.), 0.5),
        1.,
        "never smaller than itself"
    );
    assert_eq!(raster_factor((500., 400.), 1.), 1.);
    let large = raster_factor((4000., 3000.), 4.);
    assert!(large < 4. && large >= 1.);
    assert!((4000. * large) * (3000. * large) <= MAX_ZOOMED_PIXELS * 1.001);
    assert_eq!(raster_factor((0., 0.), 3.), 1.);
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
