//! Tests of the PlantUML preview (ADR 0057): its runs, stopped when the text moves on, and the
//! mistakes PlantUML finds, placed on the lines of the document.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use birchpad_diagrams::*;

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
            message: error.to_owned(),
            included: None,
            details: None,
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

/// PlantUML's picture of a mistake with its trace: where the mistake is, then each `!include`
/// that led to it, as `[From name (line N) ]` lines.
fn traced(message: &str, trace: &[(&str, usize)]) -> Vec<u8> {
    let trace: String = trace
        .iter()
        .map(|(name, line)| format!("<text>[From {name} (line {line}) ]</text>"))
        .collect();
    // Then the diagram's lines up to the mistake, the last one where it is.
    format!(
        "<svg><text>PlantUML 1.2026.9</text>{trace}<text>@startuml</text>\
         <text>$bad()</text><text>{message}</text></svg>"
    )
    .into_bytes()
}

#[test]
fn a_mistake_in_an_included_file_is_placed_on_the_include_line() {
    // `!include` on the document's fourth line, the diagram's third from `@startuml`.
    let text = "' c\n@startuml\nA -> B\n!include ../../domains/bad.iuml\nB -> C\n@enduml\n";
    let error = "Syntax Error? (Assumed diagram type: class)";
    let diagrams = [traced(
        error,
        &[("../../domains/bad.iuml", 40), ("string", 3)],
    )];
    // The error output gives the line in the included file.
    let stderr = format!("ERROR\n39\n{error}\n");
    assert_eq!(
        mistakes(&stderr, text, &diagrams),
        [Mistake {
            line: Some(3),
            message: error.to_owned(),
            included: Some(Included {
                file: "../../domains/bad.iuml".into(),
                line: 40,
                shown: Some("$bad()".into()),
                path: None,
            }),
            details: None,
        }]
    );
    // Through another included file: the last of the document's lines.
    let diagrams = [traced(
        error,
        &[
            ("bad.iuml", 3),
            ("../../domains/mid.iuml", 1),
            ("string", 3),
        ],
    )];
    let found = mistakes(
        "ERROR\n2\nSyntax Error? (Assumed diagram type: class)",
        text,
        &diagrams,
    );
    assert_eq!(found[0].line, Some(3));
    assert_eq!(found[0].included.as_ref().unwrap().file, "bad.iuml");
}

#[test]
fn a_mistake_in_an_included_file_without_the_documents_line_has_no_line() {
    let text = "@startuml\n!include lib.iuml\n@enduml\n";
    // No document in the trace: the file is named, no line of the document is.
    let diagrams = [traced("Oops", &[("lib.iuml", 7)])];
    let found = mistakes("ERROR\n6\nOops\n", text, &diagrams);
    assert_eq!(found[0].line, None);
    assert_eq!(
        found[0].included,
        Some(Included {
            file: "lib.iuml".into(),
            line: 7,
            shown: Some("$bad()".into()),
            path: None,
        })
    );
    // A trace with a line 0 for the document leads to no line either.
    let diagrams = [traced("Oops", &[("lib.iuml", 7), ("string", 0)])];
    assert_eq!(mistakes("ERROR\n6\nOops\n", text, &diagrams)[0].line, None);
    // The trace of a picture that does not show the message is not taken.
    let diagrams = [traced("Other", &[("lib.iuml", 7), ("string", 2)])];
    let found = mistakes("ERROR\n1\nOops\n", text, &diagrams);
    assert_eq!((found[0].line, found[0].included.clone()), (Some(1), None));
}

#[test]
fn the_line_under_a_mistake_is_read_from_its_picture() {
    let svg = "<svg><text>[From desc2 (line 40) ]</text><text>&#160;</text>\
               <text>@startuml</text><text>$log(&quot;a &amp; b&quot;)</text>\
               <text>Syntax Error?</text></svg>";
    assert_eq!(
        pictured_line(svg, "Syntax Error?").as_deref(),
        Some("$log(\"a & b\")")
    );
    assert_eq!(pictured_line(svg, "Other"), None, "another message");
    assert_eq!(
        pictured_line("<svg><text>Syntax Error?</text></svg>", "Syntax Error?"),
        None,
        "nothing above it"
    );
    assert_eq!(pictured_line("", "x"), None);
    assert_eq!(
        pictured_line("<svg><text>cut", "cut"),
        None,
        "a text not closed"
    );
}

/// The mistake 7.4u is about: traced only to `desc2`, PlantUML's own name of an included file.
fn untraced(line: usize, shown: &str) -> Mistake {
    Mistake {
        line: None,
        message: "Syntax Error?".into(),
        included: Some(Included {
            file: "desc2".into(),
            line,
            shown: Some(shown.into()),
            path: None,
        }),
        details: None,
    }
}

#[test]
fn a_mistake_traced_to_no_file_is_found_by_its_line() {
    let dir = tempfile::tempdir().unwrap();
    let services = dir.path().join("library").join("services");
    std::fs::create_dir_all(&services).unwrap();
    let (first, second) = (services.join("all.iuml"), services.join("billing.iuml"));
    std::fs::write(&first, "' all\n$audit_logger()\n").unwrap();
    std::fs::write(&second, "' billing\nclass A\n  $audit_logger()  \n").unwrap();
    let included = [first.clone(), second.clone()];
    // Line 3 of the second file shows it, not line 2 of the first.
    let mut found = [untraced(3, "$audit_logger()")];
    place_in_includes(&mut found, Some(dir.path()), &included);
    let place = found[0].included.clone().unwrap();
    assert_eq!(place.path, Some(second.clone()));
    assert_eq!(
        Path::new(&place.file),
        Path::new("library").join("services").join("billing.iuml"),
        "named from the document's folder"
    );
    assert_eq!(place.line, 3);
    // Without the document's folder, named by its whole path.
    let mut found = [untraced(2, "$audit_logger()")];
    place_in_includes(&mut found, None, &included);
    assert_eq!(
        found[0].included.as_ref().unwrap().file,
        first.display().to_string()
    );
}

#[test]
fn a_mistake_in_a_file_of_its_name_keeps_it() {
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("lib.iuml");
    std::fs::write(&lib, "a\nb\n").unwrap();
    let mut found = [Mistake {
        line: Some(4),
        message: "Oops".into(),
        included: Some(Included {
            file: "../lib.iuml".into(),
            line: 2,
            shown: Some("other".into()),
            path: None,
        }),
        details: None,
    }];
    place_in_includes(&mut found, Some(dir.path()), std::slice::from_ref(&lib));
    let place = found[0].included.clone().unwrap();
    assert_eq!(
        place.path,
        Some(lib),
        "by its name, whatever the line shows"
    );
    assert_eq!(place.file, "../lib.iuml", "named as PlantUML names it");
    assert_eq!(found[0].line, Some(4));
}

#[test]
fn a_mistake_found_in_no_included_file_names_none() {
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("lib.iuml");
    std::fs::write(&lib, "a\n").unwrap();
    for (line, shown) in [(1, "b"), (0, "a"), (2, "a"), (1, "")] {
        let mut found = [untraced(line, shown)];
        place_in_includes(&mut found, Some(dir.path()), std::slice::from_ref(&lib));
        let place = found[0].included.clone().unwrap();
        assert_eq!(
            (place.file.as_str(), place.path),
            ("", None),
            "{line} {shown}"
        );
    }
    // No included files, a file that cannot be read, and a mistake of the document: unchanged.
    let mut found = [untraced(1, "a")];
    place_in_includes(&mut found, None, &[dir.path().join("gone.iuml")]);
    assert_eq!(found[0].included.as_ref().unwrap().file, "");
    let mut found = [Mistake {
        line: Some(1),
        message: "x".into(),
        included: None,
        details: None,
    }];
    place_in_includes(&mut found, None, &[]);
    assert_eq!(found[0].included, None);
}

#[test]
fn a_mistake_in_the_document_stays_on_its_line() {
    let text = "@startuml\nqux\n@enduml\n";
    // Traced to the document, or without a trace (an older PlantUML): the error output's line.
    for diagrams in [
        [traced("Syntax Error?", &[("string", 2)])],
        [picture("Syntax Error?")],
    ] {
        assert_eq!(
            mistakes("ERROR\n1\nSyntax Error?\n", text, &diagrams),
            [Mistake {
                line: Some(1),
                message: "Syntax Error?".into(),
                included: None,
                details: None,
            }]
        );
    }
    // A line past any document's, after the document's first line: no line, and no panic.
    let found = mistakes(
        "ERROR
18446744073709551615
Syntax Error?
",
        "
@startuml
@enduml",
        &[picture("Syntax Error?")],
    );
    assert_eq!(found[0].line, None);
}

#[test]
fn a_mistakes_trace_is_read_from_its_picture() {
    let svg = String::from_utf8(traced(
        "x",
        &[("a &amp; b (line 2).iuml", 12), ("string", 1)],
    ))
    .unwrap();
    assert_eq!(
        mistake_trace(&svg),
        [
            ("a & b (line 2).iuml".to_owned(), 12),
            ("string".to_owned(), 1)
        ],
        "the last `(line` ends the name"
    );
    assert!(mistake_trace("").is_empty());
    assert!(mistake_trace("<svg><text>[From</text></svg>").is_empty());
    // Cut short, without a number or its parenthesis, or without a name: left out.
    for broken in [
        "[From a.iuml (line ",
        "[From a.iuml (line 3",
        "[From a.iuml (line x) ]",
        "[From  (line 3) ]",
        "[From a.iuml line 3 ]",
    ] {
        assert!(mistake_trace(broken).is_empty(), "{broken}");
    }
    assert_eq!(
        mistake_trace("[From a (line 18446744073709551615) ]"),
        [("a".to_owned(), usize::MAX)],
        "the largest line"
    );
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
            message: "Exception in thread main\nat Foo".to_owned(),
            included: None,
            details: None,
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

/// Java's error output when smetana crashes, as PlantUML writes it.
const CRASH: &str = "java.lang.NullPointerException: Cannot read field \"x\" because \"p\" is null
\tat net.sourceforge.plantuml.svek.SmetanaEdge.draw(SmetanaEdge.java:120)
\tat net.sourceforge.plantuml.svek.Smetana.run(Smetana.java:88)
";

#[test]
fn a_crash_is_one_line_with_its_trace_folded() {
    let found = mistakes(CRASH, "@startuml\nA -> B\n@enduml", &[picture("crash")]);
    assert_eq!(found.len(), 1);
    let crash = &found[0];
    assert_eq!(
        crash.message,
        "java.lang.NullPointerException: Cannot read field \"x\" because \"p\" is null"
    );
    assert_eq!(crash.line, None);
    let details = crash.details.as_deref().unwrap();
    assert!(details.contains("at net.sourceforge.plantuml.svek.SmetanaEdge.draw"));
    assert_eq!(details.lines().count(), 3);
}

#[test]
fn a_crash_is_found_after_other_lines_and_without_its_thread() {
    let stderr = "WARNING: something\nException in thread \"main\" java.lang.IllegalStateException\n\
                  \tat a.B.c(B.java:1)\nCaused by: java.lang.Error: inner\n\tat d.E.f(E.java:2)\n";
    let found = mistakes(stderr, "@startuml\n@enduml", &[]);
    assert_eq!(found[0].message, "java.lang.IllegalStateException");
    assert_eq!(
        found[0]
            .details
            .as_deref()
            .map(|details| details.lines().count()),
        Some(5)
    );
    // A thread's name without its quotes' end is left as it is.
    let found = mistakes(
        "Exception in thread \"main java.lang.Oops\n\tat a.B.c(B.java:1)\n",
        "",
        &[],
    );
    assert_eq!(found[0].message, "\"main java.lang.Oops");
    // Frames alone: the message is what comes before them, nothing here.
    assert!(
        mistakes("\tat a.B.c(B.java:1)\n", "", &[])[0]
            .details
            .is_none()
    );
}

#[test]
fn mistakes_and_other_output_are_no_crash() {
    // PlantUML's own mistakes, even with a word like Exception in them.
    let found = mistakes(
        "ERROR\n1\nException at line\n",
        "@startuml\nqux\n@enduml",
        &[picture("Exception at line")],
    );
    assert_eq!(found[0].details, None);
    assert_eq!(found[0].line, Some(1));
    // Lines starting with `at` that are no frames.
    let found = mistakes("Cannot draw\nat the end of the line\n", "", &[]);
    assert_eq!(found[0].details, None);
    assert_eq!(found[0].message, "Cannot draw\nat the end of the line");
}

#[test]
fn a_crash_of_smetana_waits_for_graphviz_with_the_automatic_layout() {
    use birchpad_config::DiagramLayout;
    let crash = mistakes(CRASH, "@startuml\n@enduml", &[]);
    assert!(waits_for_graphviz(
        &crash,
        "smetana",
        DiagramLayout::Automatic
    ));
    // Chosen layouts are kept; Graphviz's own crash has nothing to wait for.
    assert!(!waits_for_graphviz(
        &crash,
        "smetana",
        DiagramLayout::Smetana
    ));
    assert!(!waits_for_graphviz(
        &crash,
        "Graphviz",
        DiagramLayout::Automatic
    ));
    assert!(!waits_for_graphviz(
        &crash,
        "smetana",
        DiagramLayout::Graphviz
    ));
    // A mistake of the text is no crash; no mistake neither.
    let mistake = mistakes(
        "ERROR\n1\nOops\n",
        "@startuml\nx\n@enduml",
        &[picture("Oops")],
    );
    assert!(!waits_for_graphviz(
        &mistake,
        "smetana",
        DiagramLayout::Automatic
    ));
    assert!(!waits_for_graphviz(
        &[],
        "smetana",
        DiagramLayout::Automatic
    ));
}

#[test]
fn layouts_follow_the_setting_and_what_is_installed() {
    use birchpad_config::DiagramLayout;
    let dot = PathBuf::from("dot");
    assert_eq!(
        Layout::resolve(DiagramLayout::Automatic, Some(dot.clone())),
        Layout::Graphviz(Some(dot.clone()))
    );
    assert_eq!(
        Layout::resolve(DiagramLayout::Automatic, None),
        Layout::Smetana
    );
    assert_eq!(
        Layout::resolve(DiagramLayout::Smetana, Some(dot.clone())),
        Layout::Smetana
    );
    assert_eq!(
        Layout::resolve(DiagramLayout::Graphviz, None),
        Layout::Graphviz(None),
        "PlantUML looks for it itself"
    );
    assert_eq!(
        Layout::Graphviz(Some(dot.clone())).env(),
        [(GRAPHVIZ_DOT_VARIABLE.to_owned(), dot.into_os_string())]
    );
    assert!(Layout::Graphviz(None).env().is_empty());
    assert!(Layout::Smetana.env().is_empty());
    assert_eq!(Layout::Smetana.name(), "smetana");
    assert_eq!(Layout::Graphviz(None).name(), "Graphviz");
}

#[test]
fn graphviz_is_found_by_its_variable_its_path_or_its_folder() {
    let dir = tempfile::tempdir().unwrap();
    let name = if cfg!(windows) { "dot.exe" } else { "dot" };
    let folder = |name: &str| {
        let folder = dir.path().join(name);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    };
    let (named, on_path, installed, empty) = (
        folder("named"),
        folder("bin"),
        folder("Graphviz"),
        folder("empty"),
    );
    let named_dot = named.join("my-dot");
    std::fs::write(&named_dot, "").unwrap();
    std::fs::write(on_path.join(name), "").unwrap();
    std::fs::write(installed.join(name), "").unwrap();
    let path = std::env::join_paths([&empty, &on_path]).unwrap();
    assert_eq!(
        find_dot(Some(named_dot.as_os_str()), Some(&path), &[]),
        Some(named_dot.clone()),
        "the variable first"
    );
    assert_eq!(
        find_dot(Some(empty.join("gone").as_os_str()), Some(&path), &[]),
        Some(on_path.join(name)),
        "a variable naming no file is passed over"
    );
    assert_eq!(
        find_dot(
            Some(empty.as_os_str()),
            None,
            std::slice::from_ref(&installed)
        ),
        Some(installed.join(name)),
        "a folder is no file; then where Graphviz installs itself"
    );
    assert_eq!(
        find_dot(None, Some(empty.as_os_str()), std::slice::from_ref(&empty)),
        None
    );
    assert_eq!(find_dot(None, None, &[]), None);
    assert_eq!(
        find_dot(
            Some(std::ffi::OsStr::new("")),
            Some(std::ffi::OsStr::new("")),
            &[]
        ),
        None,
        "empty ones are nothing"
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
    let run = std::thread::spawn(move || {
        run_plantuml(&program, &args, None, None, &[], "@startuml", &running)
    });
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
        run_plantuml(
            "birchpad-test-no-such-plantuml",
            &[],
            None,
            None,
            &[],
            "",
            &stop
        ),
        Err(NotDrawn::Stopped)
    );
    assert!(matches!(
        run_plantuml(
            "birchpad-test-no-such-plantuml",
            &[],
            None,
            None,
            &[],
            "",
            &Stop::default()
        ),
        Err(NotDrawn::Missing(_))
    ));
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

/// `folders` as an include path, as PlantUML reads it.
fn joined(folders: &[&Path]) -> std::ffi::OsString {
    std::env::join_paths(folders).unwrap()
}

#[test]
fn the_documents_folder_comes_first_in_the_include_path() {
    let root = std::env::temp_dir().join("birchpad-include");
    let document = root.join("views").join("context");
    let library = root.join("library");
    let setting = std::env::join_paths([library.as_path(), Path::new("../shared")]).unwrap();
    assert_eq!(
        include_path(Some(&document), setting.to_str().unwrap(), None),
        Some(joined(&[&document, &library, &document.join("../shared")])),
        "the document's folder, then the setting's, a relative one from the document's"
    );
    // Then the variable Birchpad was started with.
    let inherited = joined(&[&root.join("inherited")]);
    assert_eq!(
        include_path(Some(&document), "", Some(&inherited)),
        Some(joined(&[&document, &root.join("inherited")]))
    );
}

#[test]
fn an_include_path_leaves_out_what_cannot_be_in_it() {
    let root = std::env::temp_dir().join("birchpad-include");
    let library = root.join("library");
    // No document's folder: a relative folder has nothing to be taken from.
    let setting = std::env::join_paths([Path::new("relative"), library.as_path()]).unwrap();
    assert_eq!(
        include_path(None, setting.to_str().unwrap(), None),
        Some(joined(&[&library]))
    );
    // A folder whose name has the separator in it cannot be listed.
    let separator = if cfg!(windows) { ";" } else { ":" };
    let odd = root.join(format!("a{separator}b"));
    assert_eq!(
        include_path(Some(&odd), "", None),
        None,
        "PlantUML still runs in it"
    );
    // The same folder twice is listed once.
    let twice = std::env::join_paths([library.as_path(), library.as_path()]).unwrap();
    assert_eq!(
        include_path(
            Some(&library),
            twice.to_str().unwrap(),
            Some(twice.as_os_str())
        ),
        Some(joined(&[&library]))
    );
}

#[test]
fn an_empty_include_path_is_none() {
    assert_eq!(include_path(None, "", None), None);
    let separator = if cfg!(windows) { ";" } else { ":" };
    assert_eq!(
        include_path(None, &format!(" {separator}  {separator}"), None),
        None
    );
    assert_eq!(include_path(None, "", Some(std::ffi::OsStr::new(""))), None);
    let folder = std::env::temp_dir();
    assert_eq!(
        include_path(Some(&folder), "  ", None),
        Some(joined(&[&folder])),
        "spaces are no folder"
    );
}

#[test]
fn an_included_file_is_found_in_the_include_path() {
    let dir = tempfile::tempdir().unwrap();
    let (views, library) = (dir.path().join("views"), dir.path().join("library"));
    std::fs::create_dir_all(&views).unwrap();
    std::fs::create_dir_all(&library).unwrap();
    std::fs::write(views.join("common.iuml"), "").unwrap();
    std::fs::write(library.join("lib.iuml"), "").unwrap();
    std::fs::write(library.join("common.iuml"), "").unwrap();
    let folders = [views.clone(), library.clone()];
    assert_eq!(
        find_include("common.iuml", &folders),
        Some(views.join("common.iuml")),
        "the document's folder first"
    );
    assert_eq!(
        find_include("lib.iuml", &folders),
        Some(library.join("lib.iuml"))
    );
    assert_eq!(
        find_include("../library/lib.iuml", &folders),
        Some(library.join("lib.iuml")),
        "a relative path, without its `..`"
    );
    let absolute = library.join("lib.iuml").display().to_string();
    assert_eq!(find_include(&absolute, &[]), Some(library.join("lib.iuml")));
    assert_eq!(find_include("gone.iuml", &folders), None);
    assert_eq!(
        find_include("library", &[dir.path().to_path_buf()]),
        None,
        "a folder"
    );
    assert_eq!(find_include("lib.iuml", &[]), None);
}

#[test]
fn included_files_are_found_where_plantuml_finds_them() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let document = root.join("views").join("context");
    let shared = root.join("shared");
    let library = root.join("library");
    for folder in [&document, &shared.join("billing"), &library] {
        std::fs::create_dir_all(folder).unwrap();
    }
    // As on that repository: included from another folder, including by the document's path.
    std::fs::write(
        shared.join("context.iuml"),
        "!include ../../shared/billing/billing.puml\n!include next.iuml\n!include lib.iuml\n!include gone.iuml",
    )
    .unwrap();
    std::fs::write(shared.join("next.iuml"), "' next to the file including it").unwrap();
    std::fs::write(shared.join("billing").join("billing.puml"), "class Billing").unwrap();
    std::fs::write(library.join("lib.iuml"), "' from the include path").unwrap();
    let folders = [document.clone(), library.clone()];
    let found = walk_includes(
        "@startuml\n!include ../../shared/context.iuml\n@enduml",
        Some(&document),
        &folders,
    )
    .files;
    assert_eq!(
        found,
        [
            shared.join("context.iuml"),
            shared.join("billing").join("billing.puml"),
            shared.join("next.iuml"),
            library.join("lib.iuml"),
        ]
    );
    // Without a document's folder, only the include path is looked in.
    assert_eq!(
        walk_includes("!include lib.iuml\n!include next.iuml", None, &folders).files,
        [library.join("lib.iuml")]
    );
    assert!(
        walk_includes("@startuml\n@enduml", Some(&document), &folders)
            .files
            .is_empty()
    );
}

#[test]
fn included_files_end_in_loops_and_at_a_limit() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("a.iuml"), "!include b.iuml").unwrap();
    std::fs::write(
        root.join("b.iuml"),
        "!include ./a.iuml\n!include sub/../a.iuml",
    )
    .unwrap();
    std::fs::create_dir(root.join("sub")).unwrap();
    assert_eq!(
        walk_includes("!include a.iuml", Some(root), &[]).files,
        [root.join("a.iuml"), root.join("b.iuml")],
        "each file once"
    );
    // One past the limit.
    let mut text = String::new();
    for index in 0..=MOST_INCLUDED_FILES {
        let name = format!("{index}.iuml");
        std::fs::write(root.join(&name), "").unwrap();
        text.push_str(&format!("!include {name}\n"));
    }
    let found = walk_includes(&text, Some(root), &[]).files;
    assert_eq!(found.len(), MOST_INCLUDED_FILES);
    assert_eq!(found[0], root.join("0.iuml"));
}

#[test]
fn paths_are_normalized_without_the_disk() {
    assert_eq!(normalized(Path::new("a/./b/../c")), PathBuf::from("a/c"));
    assert_eq!(normalized(Path::new("../x/..")), PathBuf::from(".."));
    assert_eq!(normalized(Path::new("")), PathBuf::new());
    let root = std::env::temp_dir();
    assert_eq!(normalized(&root.join("a").join("..")), normalized(&root));
}

#[test]
fn changes_to_included_and_plantuml_files_redraw() {
    let dir = tempfile::tempdir().unwrap();
    let document = dir.path().join("flow.puml");
    let notes = dir.path().join("notes.txt");
    let included = [notes.clone()];
    assert!(redraws_for(
        &dir.path().join("common.iuml"),
        Some(&document),
        &[]
    ));
    assert!(redraws_for(
        &dir.path().join("COMMON.PUML"),
        Some(&document),
        &[]
    ));
    assert!(
        redraws_for(&notes, Some(&document), &included),
        "an included file of any name"
    );
    assert!(!redraws_for(&notes, Some(&document), &[]));
    assert!(
        !redraws_for(&document, Some(&document), &[]),
        "the document follows its text"
    );
    assert!(!redraws_for(
        &dir.path().join("flow.puml.tmp"),
        Some(&document),
        &[]
    ));
    assert!(
        redraws_for(&document, None, &[]),
        "untitled: any PlantUML file"
    );
}

#[test]
fn the_folders_of_the_document_and_its_includes_are_watched() {
    let root = std::env::temp_dir();
    let lib = root.join("lib");
    assert_eq!(
        watched_folders(
            Some(&root),
            &[root.join("a.iuml"), lib.join("b.iuml"), lib.join("c.iuml")]
        ),
        [root.clone(), lib.clone()]
    );
    assert_eq!(watched_folders(None, &[lib.join("b.iuml")]), [lib]);
    assert!(watched_folders(None, &[]).is_empty());
}

#[test]
fn fit_shrinks_a_wide_diagram_to_the_width_alone() {
    assert_eq!(fit_width_scale((2000., 100.), 500.), 0.25);
    assert_eq!(
        fit_width_scale((100., 4000.), 500.),
        1.,
        "a tall one is not shrunk"
    );
    assert_eq!(fit_width_scale((500., 10.), 500.), 1., "exactly the room");
    assert_eq!(fit_width_scale((0., 10.), 500.), 1.);
    assert_eq!(fit_width_scale((100., 10.), 0.), 1., "no room yet");
    assert_eq!(fit_width_scale((100., 10.), -1.), 1.);
}

#[test]
fn fit_page_shrinks_a_diagram_to_be_seen_whole() {
    assert_eq!(
        fit_page_scale((1000., 4000.), (500., 1000.)),
        0.25,
        "by its height"
    );
    assert_eq!(
        fit_page_scale((2000., 100.), (500., 1000.)),
        0.25,
        "by its width"
    );
    assert_eq!(
        fit_page_scale((100., 100.), (500., 1000.)),
        1.,
        "never enlarged"
    );
    assert_eq!(
        fit_page_scale((500., 1000.), (500., 1000.)),
        1.,
        "exactly the room"
    );
    assert_eq!(fit_page_scale((0., 100.), (500., 1000.)), 1.);
    assert_eq!(fit_page_scale((100., 100.), (0., 1000.)), 1., "no room yet");
    assert_eq!(fit_page_scale((100., 100.), (-5., -5.)), 1.);
    assert_eq!(Zoom::FitPage.percent(), None);
    assert_eq!(Zoom::FitPage.zoom_out(), Zoom::Percent(90));
    assert_ne!(Zoom::FitPage.label(), Zoom::Fit.label());
}

#[test]
fn a_dragged_diagram_follows_the_mouse_within_its_edges() {
    let most = (1000., 500.);
    // Dragged left and up: the diagram moves with the mouse, scrolled right and down.
    assert_eq!(
        dragged_offset((0., 0.), (300., 300.), (200., 250.), most),
        (-100., -50.)
    );
    assert_eq!(
        dragged_offset((-100., -50.), (0., 0.), (40., 10.), most),
        (-60., -40.),
        "back"
    );
    // No further than the edges, either way.
    assert_eq!(
        dragged_offset((-10., -10.), (0., 0.), (100., 100.), most),
        (0., 0.)
    );
    assert_eq!(
        dragged_offset((0., 0.), (5000., 5000.), (0., 0.), most),
        (-1000., -500.)
    );
    // Nothing to scroll: it stays.
    assert_eq!(
        dragged_offset((0., 0.), (0., 0.), (-50., -50.), (0., 0.)),
        (0., 0.)
    );
    assert_eq!(
        dragged_offset((0., 0.), (0., 0.), (-50., -50.), (-3., -3.)),
        (0., 0.),
        "a negative most is none"
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
    // A million pixels at 400%: drawn at twice its size, the most pixels there may be.
    let large = raster_factor((1000., 1000.), 4.);
    assert!((1. ..4.).contains(&large), "{large}");
    assert!((1000. * large) * (1000. * large) <= MAX_ZOOMED_PIXELS * 1.001);
    // Past the limit at its own size: not drawn again.
    assert_eq!(raster_factor((4000., 3000.), 4.), 1.);
    assert_eq!(raster_factor((0., 0.), 3.), 1.);
}

#[test]
fn exported_diagrams_are_named_after_the_document() {
    assert_eq!(export_name(Some("flow"), 0, "png"), "flow.png");
    assert_eq!(export_name(Some("flow"), 1, "svg"), "flow-2.svg");
    assert_eq!(export_name(Some("a.b"), 9, "png"), "a.b-10.png");
    assert_eq!(export_name(None, 0, "png"), "diagram.png", "untitled");
    assert_eq!(export_name(Some(""), 0, "png"), "diagram.png");
}

/// A 4 by 2 picture: red on its left half, nothing on its right.
const HALF_RED: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="2" viewBox="0 0 4 2"><rect width="2" height="2" fill="#ff0000"/></svg>"##;

fn pixel(pixels: &(u32, u32, Vec<u8>), x: u32, y: u32) -> [u8; 4] {
    let at = ((y * pixels.0 + x) * 4) as usize;
    pixels.2[at..at + 4].try_into().unwrap()
}

#[test]
fn diagrams_are_drawn_at_a_scale_on_their_background() {
    let drawn = svg_to_pixels(HALF_RED, 2., Some([255, 255, 255, 255])).unwrap();
    assert_eq!((drawn.0, drawn.1), (8, 4));
    assert_eq!(pixel(&drawn, 1, 1), [255, 0, 0, 255]);
    assert_eq!(pixel(&drawn, 6, 2), [255, 255, 255, 255], "white");
    let drawn = svg_to_pixels(HALF_RED, 1., None).unwrap();
    assert_eq!((drawn.0, drawn.1), (4, 2));
    assert_eq!(pixel(&drawn, 3, 1)[3], 0, "transparent");
    let drawn = svg_to_pixels(HALF_RED, 8., None).unwrap();
    assert_eq!((drawn.0, drawn.1), (32, 16));
}

#[test]
fn a_huge_scale_is_lowered_and_no_picture_is_refused() {
    let wide = br#"<svg xmlns="http://www.w3.org/2000/svg" width="10000" height="10"><rect width="10" height="10"/></svg>"#;
    let drawn = svg_to_pixels(wide, 8., None).unwrap();
    assert!(drawn.0 as f32 <= MAX_PNG_SIDE, "{}", drawn.0);
    assert!(svg_to_pixels(b"not an svg", 2., None).is_err());
    assert!(svg_to_pixels(HALF_RED, 0., None).is_err());
    assert!(svg_to_pixels(HALF_RED, f32::NAN, None).is_err());
}

/// The width and height a PNG file says it has.
fn png_size(png: &[u8]) -> (u32, u32) {
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let number = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().unwrap());
    (number(16), number(20))
}

#[test]
fn a_png_is_the_picture_encoded() {
    assert_eq!(png_size(&svg_to_png(HALF_RED, 3., None).unwrap()), (12, 6));
}

#[test]
fn included_files_are_named_relative_to_the_document() {
    let root = std::env::temp_dir().join("birchpad-relative");
    let document = root.join("views").join("context");
    let file = root.join("parts").join("billing").join("containers.puml");
    assert_eq!(
        relative_to(&file, &document),
        Some(
            Path::new("..")
                .join("..")
                .join("parts")
                .join("billing")
                .join("containers.puml")
        ),
        "above the document"
    );
    assert_eq!(
        relative_to(&document.join("a.iuml"), &document),
        Some(PathBuf::from("a.iuml")),
        "beside it"
    );
    assert_eq!(
        relative_to(
            &document.join("lib").join("..").join("b.iuml"),
            &document.join(".")
        ),
        Some(PathBuf::from("b.iuml")),
        "without . and .."
    );
    assert_eq!(relative_to(&document, &document), None, "the folder itself");
    if cfg!(windows) {
        assert_eq!(
            relative_to(Path::new(r"D:\lib\a.iuml"), Path::new(r"C:\views")),
            None,
            "another drive"
        );
    }
}

#[test]
fn a_mistake_above_the_document_is_named_relative_to_it() {
    let dir = tempfile::tempdir().unwrap();
    let document = dir.path().join("views").join("context");
    let billing = dir.path().join("parts").join("billing");
    std::fs::create_dir_all(&document).unwrap();
    std::fs::create_dir_all(&billing).unwrap();
    let file = billing.join("containers.puml");
    std::fs::write(&file, "a\n$audit_logger()\n").unwrap();
    let mut found = [untraced(2, "$audit_logger()")];
    place_in_includes(&mut found, Some(&document), std::slice::from_ref(&file));
    let place = found[0].included.clone().unwrap();
    assert_eq!(
        Path::new(&place.file),
        Path::new("..")
            .join("..")
            .join("parts")
            .join("billing")
            .join("containers.puml")
    );
    assert_eq!(place.path, Some(file));
}

fn mistake_at(line: Option<usize>, message: &str, path: Option<PathBuf>) -> Mistake {
    Mistake {
        line,
        message: message.into(),
        included: path.map(|path| Included {
            file: "lib.iuml".into(),
            line: 1,
            shown: None,
            path: Some(path),
        }),
        details: None,
    }
}

#[test]
fn pictures_of_placed_mistakes_are_folded() {
    let diagrams = [
        b"<svg>A B</svg>".to_vec(),
        picture("Syntax Error?"),
        picture("Oops"),
        picture("cannot &lt;draw>"),
    ];
    let found = [
        mistake_at(Some(3), "Syntax Error?", None),
        mistake_at(None, "Oops", Some(PathBuf::from("lib.iuml"))),
        mistake_at(Some(9), "cannot <draw>", None),
    ];
    assert_eq!(
        folded_pictures(&found, &diagrams),
        [1, 2, 3],
        "by a line, or by a file"
    );
    // A mistake with no place keeps its picture; a crash's is its own.
    let unplaced = [mistake_at(None, "Oops", None)];
    assert!(folded_pictures(&unplaced, &diagrams).is_empty());
    let crash = mistakes(CRASH, "@startuml\n@enduml", &[picture("crash")]);
    assert!(folded_pictures(&crash, &[picture("crash")]).is_empty());
    // A message no picture shows folds nothing; nothing folds nothing.
    assert!(folded_pictures(&[mistake_at(Some(1), "elsewhere", None)], &diagrams).is_empty());
    assert!(folded_pictures(&[], &diagrams).is_empty());
    assert!(folded_pictures(&found, &[]).is_empty());
    // Two mistakes of the same message: one picture each, in order.
    let twice = [
        mistake_at(Some(1), "Syntax Error?", None),
        mistake_at(None, "Syntax Error?", None),
    ];
    let both = [picture("Syntax Error?"), picture("Syntax Error?")];
    assert_eq!(
        folded_pictures(&twice, &both),
        [0],
        "the second has no place"
    );
}

#[test]
fn a_crash_of_smetana_is_drawn_again_with_elk_with_the_automatic_layout() {
    use birchpad_config::DiagramLayout;
    let crash = mistakes(CRASH, "@startuml\n@enduml", &[]);
    assert!(draws_again_with_elk(
        &crash,
        &Layout::Smetana,
        DiagramLayout::Automatic
    ));
    // A layout chosen is kept; Graphviz's and ELK's crashes are their own.
    for setting in [
        DiagramLayout::Smetana,
        DiagramLayout::Graphviz,
        DiagramLayout::Elk,
    ] {
        assert!(
            !draws_again_with_elk(&crash, &Layout::Smetana, setting),
            "{setting:?}"
        );
    }
    assert!(!draws_again_with_elk(
        &crash,
        &Layout::Graphviz(None),
        DiagramLayout::Automatic
    ));
    assert!(!draws_again_with_elk(
        &crash,
        &Layout::Elk,
        DiagramLayout::Automatic
    ));
    // A mistake of the text, or none, is no crash.
    let mistake = mistakes(
        "ERROR\n1\nOops\n",
        "@startuml\nx\n@enduml",
        &[picture("Oops")],
    );
    assert!(!draws_again_with_elk(
        &mistake,
        &Layout::Smetana,
        DiagramLayout::Automatic
    ));
    assert!(!draws_again_with_elk(
        &[],
        &Layout::Smetana,
        DiagramLayout::Automatic
    ));
    assert_eq!(Layout::resolve(DiagramLayout::Elk, None), Layout::Elk);
    assert_eq!(
        Layout::resolve(DiagramLayout::Elk, Some(PathBuf::from("dot"))),
        Layout::Elk,
        "ELK chosen, even with Graphviz there"
    );
    assert_eq!(Layout::Elk.name(), "ELK");
    assert!(Layout::Elk.env().is_empty());
    let (_, args) = plantuml_command("plantuml", None, &Layout::Elk);
    assert_eq!(args.last().map(String::as_str), Some("-Playout=elk"));
}

/// Java's error output when PlantUML has no ELK and is asked for it.
const NO_ELK: &str = "java.lang.ClassNotFoundException: org.eclipse.elk.graph.util.ElkGraphUtil\n\
                      \tat java.base/jdk.internal.loader.BuiltinClassLoader.loadClass(Unknown Source)\n";

/// Java's error output when ELK crashes, as on relations to groups once unlinked ones go.
const ELK_CRASH: &str = "java.lang.NullPointerException: \"node1\" is null\n\
                         \tat org.eclipse.elk.graph.util.ElkGraphUtil.createSimpleEdge(ElkGraphUtil.java:200)\n";

fn drawn_with(stderr: &str) -> Result<Drawn, NotDrawn> {
    Ok(Drawn {
        diagrams: vec![picture("crash")],
        mistakes: mistakes(stderr, "@startuml\n@enduml", &[picture("crash")]),
    })
}

#[test]
fn what_came_of_elk_is_told_apart() {
    assert_eq!(elk_attempt(&drawn_with("")), ElkAttempt::Drew);
    assert_eq!(elk_attempt(&drawn_with(ELK_CRASH)), ElkAttempt::Crashed);
    assert_eq!(
        elk_attempt(&drawn_with(NO_ELK)),
        ElkAttempt::Missing,
        "not there"
    );
    assert_eq!(
        elk_attempt(&Err(NotDrawn::Failed("no diagram".into()))),
        ElkAttempt::Crashed,
        "drew nothing"
    );
    assert_eq!(ElkAttempt::default(), ElkAttempt::NotTried);
    assert!(lacks_elk(&mistakes(NO_ELK, "", &[])));
    assert!(
        !lacks_elk(&mistakes(ELK_CRASH, "", &[])),
        "ELK is there; it crashed"
    );
    // A class of another library missing is no missing ELK.
    let other = "java.lang.ClassNotFoundException: org.other.Thing\n\tat a.B.c(B.java:1)\n";
    assert!(!lacks_elk(&mistakes(other, "", &[])));
    assert!(!lacks_elk(&[]));
}

#[test]
fn pictures_of_crashes_are_found_by_their_exception() {
    let crash = mistakes(CRASH, "@startuml\n@enduml", &[]);
    let diagrams = [
        b"<svg>A B</svg>".to_vec(),
        b"<svg><text>An error has occurred : java.lang.NullPointerException: x</text></svg>"
            .to_vec(),
    ];
    assert_eq!(crash_pictures(&crash, &diagrams), [1]);
    // A picture that names no exception, a mistake that is no crash: nothing.
    assert!(crash_pictures(&crash, &[picture("crash")]).is_empty());
    let mistake = mistakes(
        "ERROR\n1\nOops\n",
        "@startuml\nx\n@enduml",
        &[picture("Oops")],
    );
    assert!(crash_pictures(&mistake, &[picture("Oops")]).is_empty());
    assert!(crash_pictures(&[], &diagrams).is_empty());
    assert!(crash_pictures(&crash, &[]).is_empty());
    // Two crashes, two pictures, in order.
    let twice: Vec<Mistake> = crash.iter().chain(&crash).cloned().collect();
    let both = [diagrams[1].clone(), diagrams[1].clone()];
    assert_eq!(crash_pictures(&twice, &both), [0, 1]);
}
