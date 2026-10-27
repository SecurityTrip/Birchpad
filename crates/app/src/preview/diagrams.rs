//! The preview of a PlantUML document (ADR 0057): PlantUML draws the text as SVG in the
//! background once it has rested, a run the text has moved past is stopped, and its mistakes
//! are listed by the line of the document they are on, each a link to that line.

use std::io::{Read as _, Write as _};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// What PlantUML writes after each diagram, to tell the diagrams of a file apart.
pub const DIAGRAM_END: &str = "<!--birchpad-diagram-end-->";

/// How long the text rests before PlantUML draws it again: it starts a Java program each time.
pub const PLANTUML_DELAY: Duration = Duration::from_millis(400);

/// The program and arguments that draw PlantUML's diagrams as SVG from the standard input, for
/// the `preview.plantuml` setting: a `.jar` runs with `java -jar`. With the setting as it comes
/// (`plantuml`) and PlantUML installed by Birchpad in `installed`, that one runs, with its own
/// Java and the layout that needs no Graphviz.
pub fn plantuml_command(setting: &str, installed: Option<&Path>) -> (String, Vec<String>) {
    let setting = match setting.trim() {
        "" => "plantuml",
        setting => setting,
    };
    let options = [
        "-tsvg",
        "-pipe",
        "-charset",
        "UTF-8",
        "-pipedelimitor",
        DIAGRAM_END,
    ]
    .map(str::to_owned);
    if let Some(folder) = installed.filter(|_| setting == "plantuml") {
        let java =
            folder
                .join("jre")
                .join("bin")
                .join(if cfg!(windows) { "java.exe" } else { "java" });
        let mut args = vec![
            "-Djava.awt.headless=true".to_owned(),
            "-jar".to_owned(),
            folder.join("plantuml.jar").display().to_string(),
        ];
        args.extend(options);
        args.push("-Playout=smetana".to_owned());
        return (java.display().to_string(), args);
    }
    let is_jar = Path::new(setting)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("jar"));
    if is_jar {
        let mut args = vec!["-jar".to_owned(), setting.to_owned()];
        args.extend(options);
        ("java".to_owned(), args)
    } else {
        (setting.to_owned(), options.to_vec())
    }
}

/// The `preview.plantuml` setting with a relative path in it taken from `base`, Birchpad's
/// working folder, since PlantUML runs in the document's folder: a `.jar` (`plantuml.jar`,
/// `tools/plantuml.jar`) or a program by a path (`bin/plantuml`). A program's name alone
/// (`plantuml`) is looked for on `PATH`, and stays.
pub fn setting_path(setting: &str, base: &Path) -> String {
    let setting = setting.trim();
    let path = Path::new(setting);
    let is_jar = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("jar"));
    let has_folder = setting.contains('/') || setting.contains(std::path::MAIN_SEPARATOR);
    if setting.is_empty() || path.is_absolute() || !(is_jar || has_folder) {
        return setting.to_owned();
    }
    base.join(path).display().to_string()
}

/// The SVG images in PlantUML's output, one per diagram.
pub fn split_diagrams(output: &[u8]) -> Vec<Vec<u8>> {
    String::from_utf8_lossy(output)
        .split(DIAGRAM_END)
        .map(str::trim)
        .filter(|part| part.contains("<svg"))
        .map(|part| part.as_bytes().to_vec())
        .collect()
}

/// A mistake PlantUML found: what it says, and the line of the document it is on (from 0),
/// when that is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mistake {
    pub line: Option<usize>,
    pub message: String,
}

/// The lines (from 0) where the diagrams of `text` start: `@startuml`, `@startmindmap` and the
/// other `@start` lines.
pub fn diagram_starts(text: &str) -> Vec<usize> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| {
            let line = line.trim_start();
            line.get(..6)
                .is_some_and(|start| start.eq_ignore_ascii_case("@start"))
        })
        .map(|(number, _)| number)
        .collect()
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;")
}

/// The mistakes in PlantUML's error output: for each diagram with mistakes, `ERROR`, the line
/// in the diagram (from its `@start` line, 0) and what is wrong, in the order of the diagrams.
/// Each is placed in the document `text` by the diagram whose picture (in `diagrams`) shows its
/// message. Output in another form is one mistake, without a line.
pub fn mistakes(stderr: &str, text: &str, diagrams: &[Vec<u8>]) -> Vec<Mistake> {
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let starts = diagram_starts(text);
    let mut found = Vec::new();
    // The diagram after the one the last mistake was in.
    let mut next = 0;
    let mut at = 0;
    while at < lines.len() {
        let (true, Some(Ok(line)), Some(message)) = (
            lines[at] == "ERROR",
            lines.get(at + 1).map(|line| line.parse::<usize>()),
            lines.get(at + 2),
        ) else {
            break;
        };
        let shown = escape_xml(message);
        let diagram = (next..diagrams.len())
            .find(|&index| String::from_utf8_lossy(&diagrams[index]).contains(&shown))
            .unwrap_or(next);
        next = diagram + 1;
        found.push(Mistake {
            line: starts.get(diagram).map(|start| start + line),
            message: (*message).to_owned(),
        });
        at += 3;
    }
    if found.is_empty() && !lines.is_empty() {
        found.push(Mistake {
            line: None,
            message: lines.join("\n"),
        });
    }
    found
}

/// Where line `line` (from 0) of `text` starts, in bytes; the end of the text past its last
/// line. `\n`, `\r\n` and `\r` end lines.
pub fn line_offset(text: &str, line: usize) -> usize {
    let bytes = text.as_bytes();
    let mut current = 0;
    let mut at = 0;
    while current < line {
        match bytes[at..].iter().position(|&b| b == b'\n' || b == b'\r') {
            Some(end) => {
                at += end;
                at += if bytes[at..].starts_with(b"\r\n") {
                    2
                } else {
                    1
                };
                current += 1;
            }
            None => return text.len(),
        }
    }
    at
}

/// Why PlantUML drew nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotDrawn {
    /// There is no PlantUML to run: Birchpad can install it.
    Missing(String),
    Failed(String),
    /// The run was stopped: the text changed since.
    Stopped,
}

/// What PlantUML drew: the diagrams (a diagram with mistakes is a picture of them), and the
/// mistakes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Drawn {
    pub diagrams: Vec<Vec<u8>>,
    pub mistakes: Vec<Mistake>,
}

/// Stops a PlantUML run: the process is ended, and a run not started yet does not start.
#[derive(Clone, Default)]
pub struct Stop(Arc<Mutex<(bool, Option<Child>)>>);

impl Stop {
    pub fn stop(&self) {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        state.0 = true;
        if let Some(mut child) = state.1.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    pub fn is_stopped(&self) -> bool {
        self.0.lock().unwrap_or_else(|error| error.into_inner()).0
    }

    /// Keeps `child` to be stopped; false, and the child ended, if the run was stopped already.
    fn keep(&self, mut child: Child) -> bool {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if state.0 {
            let _ = child.kill();
            let _ = child.wait();
            return false;
        }
        state.1 = Some(child);
        true
    }

    /// The child back once its output is read, unless it was stopped meanwhile.
    fn take(&self) -> Option<Child> {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .1
            .take()
    }
}

/// Stops its run when it is dropped: the drawing a newer one replaces.
pub struct StopOnDrop(pub Stop);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.stop();
    }
}

/// Runs `program` with `args` on `text`, as PlantUML, in `folder` (the document's, so that
/// `!include` and images by relative path are found next to it): the diagrams it wrote and its
/// mistakes, or why there are none. `stop` ends the run early.
pub fn run_plantuml(
    program: &str,
    args: &[String],
    folder: Option<&Path>,
    text: &str,
    stop: &Stop,
) -> Result<Drawn, NotDrawn> {
    if stop.is_stopped() {
        return Err(NotDrawn::Stopped);
    }
    let mut command = Command::new(program);
    // A folder that is gone leaves PlantUML in Birchpad's, rather than not running it.
    if let Some(folder) = folder.filter(|folder| folder.is_dir()) {
        command.current_dir(folder);
    }
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        // CREATE_NO_WINDOW: no console flashes up for Java.
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            NotDrawn::Missing(t!("preview.plantuml-not-found", program = program))
        } else {
            NotDrawn::Failed(t!("preview.plantuml-failed", error = error))
        }
    })?;
    let mut stdin = child.stdin.take().expect("piped");
    let mut stdout = child.stdout.take().expect("piped");
    let mut stderr = child.stderr.take().expect("piped");
    if !stop.keep(child) {
        return Err(NotDrawn::Stopped);
    }
    let input = text.to_owned();
    // Written and read in other threads, so that a large diagram cannot block on a full pipe.
    let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
    let errors = std::thread::spawn(move || {
        let mut errors = Vec::new();
        let _ = stderr.read_to_end(&mut errors);
        errors
    });
    let mut output = Vec::new();
    let read = stdout.read_to_end(&mut output);
    let errors = errors.join().unwrap_or_default();
    let _ = writer.join();
    let Some(mut child) = stop.take() else {
        return Err(NotDrawn::Stopped);
    };
    let succeeded = child.wait().is_ok_and(|status| status.success());
    read.map_err(|error| NotDrawn::Failed(t!("preview.plantuml-failed", error = error)))?;
    // A diagram with mistakes is drawn as a picture of them, with a status that is not 0.
    let diagrams = split_diagrams(&output);
    let errors = String::from_utf8_lossy(&errors);
    if diagrams.is_empty() {
        let message = errors.trim().to_owned();
        return Err(NotDrawn::Failed(if message.is_empty() {
            t!("preview.plantuml-no-diagram")
        } else {
            message
        }));
    }
    // What a run that succeeded writes there is no mistake: Java's warnings.
    let mistakes = if succeeded {
        Vec::new()
    } else {
        mistakes(&errors, text, &diagrams)
    };
    Ok(Drawn { diagrams, mistakes })
}
