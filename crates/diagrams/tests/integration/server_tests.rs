//! Tests of PlantUML kept running (ADR 0060): each drawing given in turn to one process, the
//! drawings' pictures and errors told apart, and a process stopped with its drawing not kept.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use birchpad_diagrams::server::*;
use birchpad_diagrams::{DIAGRAM_END, Mistake, NotDrawn, Stop, run_plantuml};

#[test]
fn java_is_kept_running_and_nothing_else() {
    for program in [
        "java",
        "java.exe",
        "JAVA.EXE",
        "/usr/bin/java",
        r"C:\jre\bin\java.exe",
    ] {
        assert!(keeps_running(program), "{program}");
    }
    for program in [
        "plantuml",
        "plantuml.cmd",
        "javaw.exe",
        "my-java",
        "",
        "/usr/bin/",
        r"C:\jre\bin\",
    ] {
        assert!(!keeps_running(program), "{program}");
    }
}

#[test]
fn diagrams_must_end_before_the_next_starts() {
    assert!(complete_diagrams("@startuml\nA -> B\n@enduml\n"));
    assert!(complete_diagrams(
        "' a comment\n@startuml\n@enduml\n\n  @STARTMINDMAP\n* a\n  @endmindmap"
    ));
    assert!(complete_diagrams(""), "no diagram, nothing left open");
    assert!(complete_diagrams("A -> B"));
    assert!(!complete_diagrams("@startuml\nA -> B"), "being typed");
    assert!(!complete_diagrams("@startuml\n@startuml\n@enduml\n@enduml"));
    assert!(
        !complete_diagrams("@enduml\n@startuml\n@enduml"),
        "an end first"
    );
    assert!(
        complete_diagrams("@startuml\n@enduml\n@star"),
        "@star is no start: nothing is left open"
    );
}

#[test]
fn a_drawings_end_fails_with_its_marker() {
    let end = end_of_drawing("birchpad-drawing-7");
    assert!(
        end.starts_with("\n@startuml\n"),
        "after a text without a line end too"
    );
    assert!(end.contains("!assert") && end.contains("\"birchpad-drawing-7\""));
    assert!(end.ends_with("@enduml\n"));
}

/// A stand-in for PlantUML run with `-pipe`: for each diagram, a picture with its process's
/// number and text; `bad` in it is a mistake, `slow` takes a minute or more; the end of a
/// drawing fails with its marker.
fn kept_plantuml(dir: &Path) -> (String, Vec<String>) {
    if cfg!(windows) {
        let script = dir.join("kept-plantuml.ps1");
        std::fs::write(
            &script,
            format!(
                r#"$end = '{DIAGRAM_END}'
$body = ''
while ($null -ne ($line = [Console]::In.ReadLine())) {{
  if ($line -like '@start*') {{ $body = '' }}
  elseif ($line -like '@end*') {{
    if ($body -like '*!assert*') {{
      $marker = ($body -split '"')[1]
      [Console]::Out.Write("<svg><text>Assertion error : $marker</text></svg>$end`n")
      [Console]::Error.Write("ERROR`n1`nAssertion error : $marker`n")
    }} elseif ($body -like '*slow*') {{
      Start-Sleep -Seconds 60
    }} elseif ($body -like '*bad*') {{
      [Console]::Out.Write("<svg><text>Syntax Error?</text></svg>$end`n")
      [Console]::Error.Write("ERROR`n1`nSyntax Error?`n")
    }} else {{
      [Console]::Out.Write("<svg>$PID$body</svg>$end`n")
    }}
    [Console]::Out.Flush()
    [Console]::Error.Flush()
  }} else {{ $body += ' ' + $line }}
}}
"#
            ),
        )
        .unwrap();
        (
            "powershell".into(),
            [
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                &script.display().to_string(),
            ]
            .map(String::from)
            .to_vec(),
        )
    } else {
        let script = dir.join("kept-plantuml.sh");
        std::fs::write(
            &script,
            format!(
                r#"end='{DIAGRAM_END}'
body=''
while IFS= read -r line; do
  case "$line" in
    @start*) body='' ;;
    @end*)
      case "$body" in
        *'!assert'*)
          marker=${{body#*\"}}; marker=${{marker%%\"*}}
          printf '<svg><text>Assertion error : %s</text></svg>%s\n' "$marker" "$end"
          printf 'ERROR\n1\nAssertion error : %s\n' "$marker" >&2 ;;
        *slow*)
          i=0; while [ "$i" -lt 100000000 ]; do i=$((i + 1)); done ;;
        *bad*)
          printf '<svg><text>Syntax Error?</text></svg>%s\n' "$end"
          printf 'ERROR\n1\nSyntax Error?\n' >&2 ;;
        *) printf '<svg>%s%s</svg>%s\n' "$$" "$body" "$end" ;;
      esac ;;
    *) body="$body $line" ;;
  esac
done
"#
            ),
        )
        .unwrap();
        ("sh".into(), vec![script.display().to_string()])
    }
}

fn draw_with(
    (program, args): &(String, Vec<String>),
    folder: &Path,
    text: &str,
) -> Option<Result<birchpad_diagrams::Drawn, NotDrawn>> {
    draw_kept(program, args, Some(folder), &[], text, &Stop::default())
}

/// The process that drew a picture of the stand-in, and the text it shows.
fn drawn_by(svg: &[u8]) -> (String, String) {
    let svg = String::from_utf8_lossy(svg);
    let inner = svg.trim_start_matches("<svg>").trim_end_matches("</svg>");
    let (process, text) = inner.split_once(' ').unwrap_or((inner, ""));
    (process.to_owned(), text.trim().to_owned())
}

#[test]
fn one_process_draws_each_drawing_in_turn() {
    let dir = tempfile::tempdir().unwrap();
    let fake = kept_plantuml(dir.path());
    let first = draw_with(&fake, dir.path(), "@startuml\nA -> B\n@enduml\n")
        .expect("kept")
        .unwrap();
    assert_eq!(first.diagrams.len(), 1);
    assert!(first.mistakes.is_empty());
    let (process, text) = drawn_by(&first.diagrams[0]);
    assert_eq!(text, "A -> B");
    // Two diagrams, the last line without its end.
    let second = draw_with(
        &fake,
        dir.path(),
        "@startuml\nC\n@enduml\n@startuml\nD\n@enduml",
    )
    .expect("kept")
    .unwrap();
    let drawn: Vec<_> = second.diagrams.iter().map(|svg| drawn_by(svg)).collect();
    assert_eq!(
        drawn,
        [(process.clone(), "C".into()), (process.clone(), "D".into())],
        "the same process"
    );
    assert!(kept_folders().contains(&Some(dir.path().to_path_buf())));
}

#[test]
fn each_drawing_has_its_own_mistakes() {
    let dir = tempfile::tempdir().unwrap();
    let fake = kept_plantuml(dir.path());
    let text = "' first\n@startuml\nbad\n@enduml\n";
    let drawn = draw_with(&fake, dir.path(), text).expect("kept").unwrap();
    assert_eq!(
        drawn.mistakes,
        [Mistake {
            line: Some(2),
            message: "Syntax Error?".into(),
            included: None,
            details: None,
        }]
    );
    // The next drawing's error output is its own: none.
    let drawn = draw_with(&fake, dir.path(), "@startuml\nA\n@enduml\n")
        .expect("kept")
        .unwrap();
    assert!(drawn.mistakes.is_empty(), "{:?}", drawn.mistakes);
    // Nothing to draw: said as one-shot runs say it.
    let drawn = draw_with(&fake, dir.path(), "' no diagram\n").expect("kept");
    assert!(matches!(drawn, Err(NotDrawn::Failed(_))), "{drawn:?}");
}

#[test]
fn a_stopped_drawing_ends_its_process() {
    let dir = tempfile::tempdir().unwrap();
    let fake = kept_plantuml(dir.path());
    let first = draw_with(&fake, dir.path(), "@startuml\nA\n@enduml\n")
        .expect("kept")
        .unwrap();
    let (process, _) = drawn_by(&first.diagrams[0]);
    let stop = Stop::default();
    stop.stop();
    assert_eq!(
        draw_kept(
            &fake.0,
            &fake.1,
            Some(dir.path()),
            &[],
            "@startuml\nA\n@enduml",
            &stop
        ),
        Some(Err(NotDrawn::Stopped))
    );
    // Stopped while it draws.
    let stop = Stop::default();
    let stopping = stop.clone();
    let (program, args, folder) = (fake.0.clone(), fake.1.clone(), dir.path().to_path_buf());
    let started = Instant::now();
    let drawing = std::thread::spawn(move || {
        draw_kept(
            &program,
            &args,
            Some(&folder),
            &[],
            "@startuml\nslow\n@enduml\n",
            &stopping,
        )
    });
    std::thread::sleep(Duration::from_millis(300));
    stop.stop();
    assert_eq!(
        drawing.join().unwrap(),
        None,
        "the caller sees the run stopped"
    );
    assert!(started.elapsed() < Duration::from_secs(20));
    // The next drawing starts another process.
    let next = draw_with(&fake, dir.path(), "@startuml\nA\n@enduml\n")
        .expect("kept")
        .unwrap();
    assert_ne!(drawn_by(&next.diagrams[0]).0, process);
}

#[test]
fn a_program_that_does_not_run_is_not_kept() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        draw_kept(
            "birchpad-test-no-such-plantuml",
            &[],
            Some(dir.path()),
            &[],
            "@startuml\n@enduml",
            &Stop::default()
        ),
        None
    );
    // One that ends at once, without a picture.
    let (program, args) = if cfg!(windows) {
        ("cmd".to_owned(), vec!["/c".to_owned(), "exit 0".to_owned()])
    } else {
        ("sh".to_owned(), vec!["-c".to_owned(), "exit 0".to_owned()])
    };
    assert_eq!(
        draw_kept(
            &program,
            &args,
            Some(dir.path()),
            &[],
            "@startuml\n@enduml",
            &Stop::default()
        ),
        None
    );
    assert!(!kept_folders().contains(&Some(dir.path().to_path_buf())));
}

#[test]
fn only_the_last_ways_of_running_are_kept() {
    let dir = tempfile::tempdir().unwrap();
    let fake = kept_plantuml(dir.path());
    let folders: Vec<PathBuf> = (0..=MOST_KEPT)
        .map(|index| {
            let folder = dir.path().join(index.to_string());
            std::fs::create_dir(&folder).unwrap();
            folder
        })
        .collect();
    for folder in &folders {
        draw_with(&fake, folder, "@startuml\nA\n@enduml\n")
            .expect("kept")
            .unwrap();
    }
    let kept = kept_folders();
    assert!(
        !kept.contains(&Some(folders[0].clone())),
        "the oldest ended"
    );
    assert!(kept.contains(&Some(folders[MOST_KEPT].clone())));
    assert!(kept.len() <= MOST_KEPT);
}

/// PlantUML as Birchpad installs it, if it is installed here.
fn installed_plantuml() -> Option<(String, Vec<String>)> {
    let data = std::env::var_os(if cfg!(windows) {
        "LOCALAPPDATA"
    } else {
        "HOME"
    })?;
    let components = PathBuf::from(data).join("Birchpad").join("components");
    let folder = std::fs::read_dir(components)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("plantuml-"))
        })?;
    Some(birchpad_diagrams::plantuml_command(
        "plantuml",
        Some(&folder),
        &birchpad_diagrams::Layout::Smetana,
    ))
}

#[test]
#[ignore = "a timing, with the PlantUML Birchpad installs; run in release"]
fn a_kept_plantuml_draws_faster() {
    let Some((program, args)) = installed_plantuml() else {
        eprintln!("no PlantUML installed by Birchpad: nothing measured");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let texts = [
        "@startuml\nAlice -> Bob: hello\nBob --> Alice: hi\n@enduml\n",
        "@startuml\nclass A\nclass B\nA --> B\n@enduml\n",
    ];
    let mut started = Duration::ZERO;
    let mut kept = Duration::ZERO;
    for text in texts {
        let at = Instant::now();
        run_plantuml(
            &program,
            &args,
            Some(dir.path()),
            None,
            &[],
            text,
            &Stop::default(),
        )
        .unwrap();
        started += at.elapsed();
        // The first drawing starts the process; the second is what typing costs.
        draw_with(&(program.clone(), args.clone()), dir.path(), text)
            .unwrap()
            .unwrap();
        let at = Instant::now();
        draw_with(&(program.clone(), args.clone()), dir.path(), text)
            .unwrap()
            .unwrap();
        kept += at.elapsed();
    }
    eprintln!("started for each drawing: {started:?}; kept running: {kept:?}");
    assert!(kept < started, "kept only if faster");
}
