//! Tests of the PlantUML preview (ADR 0057): its runs, stopped when the text moves on, and the
//! mistakes PlantUML finds, placed on the lines of the document.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use crate::workspace::open_workspace;
use birchpad::app_state::AppState;
use birchpad::preview::*;
use birchpad::workspace::Workspace;

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
