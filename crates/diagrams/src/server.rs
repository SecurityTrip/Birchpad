//! PlantUML kept running (ADR 0060): starting Java for each drawing takes half a second before
//! PlantUML reads a line, so a PlantUML started with `-pipe` is kept for each way of running it
//! (its program, arguments, folder and variables) and given each drawing in turn. After the
//! document's text it gets a diagram that fails on purpose with a message of its own: its
//! picture ends the drawing's diagrams, and its error ends the drawing's error output.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use crate::{
    DIAGRAM_END, Drawn, INCLUDE_PATH_VARIABLE, NotDrawn, Stop, mistakes, run_plantuml,
    split_diagrams,
};

/// How many PlantUMLs are kept running at most, for documents in that many folders or ways.
pub const MOST_KEPT: usize = 4;

/// How long the error output of a drawing may lag behind its pictures.
const ERRORS_WAIT: Duration = Duration::from_secs(5);

/// What tells kept PlantUMLs apart: the same program, arguments, folder and variables.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Key {
    program: String,
    args: Vec<String>,
    folder: Option<PathBuf>,
    env: Vec<(String, OsString)>,
}

/// What PlantUML wrote to its error output, line by line, as a thread reads it.
#[derive(Default)]
struct Errors {
    lines: Mutex<(Vec<String>, bool)>,
    added: Condvar,
}

/// A PlantUML kept running.
struct Kept {
    key: Key,
    /// `None` while a drawing has it, kept by its [`Stop`].
    child: Option<Child>,
    stdin: ChildStdin,
    stdout: ChildStdout,
    /// Output read past the end of the last drawing.
    pending: Vec<u8>,
    errors: Arc<Errors>,
}

impl Drop for Kept {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// The PlantUMLs kept running, the one used last at the end.
static KEPT: Mutex<VecDeque<Kept>> = Mutex::new(VecDeque::new());

/// Tells each drawing's end from the others.
static DRAWINGS: AtomicU64 = AtomicU64::new(0);

/// Whether `program` is kept running between drawings: Java, which runs the PlantUML Birchpad
/// installs and a `plantuml.jar`. A program of another kind (a script that runs PlantUML, or
/// something else) is started for each drawing, as it may not read its input as it comes. The
/// name is read after the last `/` or `\` on every system, as a setting copied from Windows has
/// backslashes that `Path` reads as a separator only there.
pub fn keeps_running(program: &str) -> bool {
    let name = program.rsplit(['/', '\\']).next().unwrap_or(program);
    Path::new(name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem.eq_ignore_ascii_case("java"))
}

/// Whether each diagram of `text` ends (`@end...`) before the next starts and the text ends: a
/// diagram left open would take in the drawing's end, and PlantUML would wait for more.
pub fn complete_diagrams(text: &str) -> bool {
    let mut open = false;
    for line in text.lines() {
        let line = line.trim_start();
        let starts = |word: &str| {
            line.get(..word.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(word))
        };
        if starts("@start") {
            if open {
                return false;
            }
            open = true;
        } else if starts("@end") {
            if !open {
                return false;
            }
            open = false;
        }
    }
    !open
}

/// Draws `text` as [`run_plantuml`] does, with a PlantUML kept running when `program` can be
/// ([`keeps_running`]) and the text's diagrams are complete; started for this drawing
/// otherwise, or when the kept one fails.
pub fn draw(
    program: &str,
    args: &[String],
    folder: Option<&Path>,
    include_path: Option<&std::ffi::OsStr>,
    env: &[(String, OsString)],
    text: &str,
    stop: &Stop,
) -> Result<Drawn, NotDrawn> {
    if keeps_running(program) && complete_diagrams(text) {
        let mut all = env.to_vec();
        if let Some(include_path) = include_path {
            all.push((INCLUDE_PATH_VARIABLE.to_owned(), include_path.to_owned()));
        }
        if let Some(drawn) = draw_kept(program, args, folder, &all, text, stop) {
            return drawn;
        }
    }
    run_plantuml(program, args, folder, include_path, env, text, stop)
}

/// Draws `text` with the PlantUML kept for `program` and the rest, started if there is none.
/// `None` when it cannot be started or stopped working (it is not kept then): the caller
/// starts PlantUML for this drawing alone, which says what went wrong.
pub fn draw_kept(
    program: &str,
    args: &[String],
    folder: Option<&Path>,
    env: &[(String, OsString)],
    text: &str,
    stop: &Stop,
) -> Option<Result<Drawn, NotDrawn>> {
    if stop.is_stopped() {
        return Some(Err(NotDrawn::Stopped));
    }
    let key = Key {
        program: program.to_owned(),
        args: args.to_vec(),
        folder: folder
            .filter(|folder| folder.is_dir())
            .map(Path::to_path_buf),
        env: env.to_vec(),
    };
    let taken = {
        let mut kept = KEPT.lock().unwrap_or_else(|error| error.into_inner());
        let at = kept.iter().position(|kept| kept.key == key);
        at.and_then(|at| kept.remove(at))
    };
    let mut kept = match taken {
        Some(kept) => kept,
        None => start(key)?,
    };
    let child = kept.child.take().expect("a kept PlantUML has its process");
    if !stop.keep(child) {
        return Some(Err(NotDrawn::Stopped));
    }
    let drawn = drawing(&mut kept, text);
    // Stopped meanwhile: its process was ended, and it is not kept.
    let child = stop.take()?;
    kept.child = Some(child);
    let drawn = drawn?;
    let mut all = KEPT.lock().unwrap_or_else(|error| error.into_inner());
    all.push_back(kept);
    while all.len() > MOST_KEPT {
        all.pop_front();
    }
    Some(drawn)
}

/// Starts PlantUML to keep: `None` if it does not start.
fn start(key: Key) -> Option<Kept> {
    let mut command = Command::new(&key.program);
    if let Some(folder) = &key.folder {
        command.current_dir(folder);
    }
    command
        .args(&key.args)
        .envs(key.env.iter().map(|(name, value)| (name, value)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        // CREATE_NO_WINDOW: no console flashes up for Java.
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().ok()?;
    let stdin = child.stdin.take()?;
    let stdout = child.stdout.take()?;
    let stderr = child.stderr.take()?;
    let errors = Arc::new(Errors::default());
    let reading = errors.clone();
    std::thread::spawn(move || {
        let mut lines = BufReader::new(stderr).lines();
        loop {
            let line = lines.next();
            let mut state = reading
                .lines
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            match line {
                Some(Ok(line)) => state.0.push(line),
                _ => state.1 = true,
            }
            let ended = state.1;
            drop(state);
            reading.added.notify_all();
            if ended {
                break;
            }
        }
    });
    Some(Kept {
        key,
        child: Some(child),
        stdin,
        stdout,
        pending: Vec::new(),
        errors,
    })
}

/// The diagram written after a drawing's text: it fails, with `marker` as its message.
pub fn end_of_drawing(marker: &str) -> String {
    format!("\n@startuml\n!assert 1 == 2 : \"{marker}\"\n@enduml\n")
}

/// One drawing with a kept PlantUML: its text, then the end of the drawing, written; the
/// pictures read up to the end's, and the error output up to its message. `None` when the
/// process ended.
fn drawing(kept: &mut Kept, text: &str) -> Option<Result<Drawn, NotDrawn>> {
    let marker = format!(
        "birchpad-drawing-{}",
        DRAWINGS.fetch_add(1, Ordering::Relaxed)
    );
    let input = format!("{text}{}", end_of_drawing(&marker));
    let Kept {
        stdin,
        stdout,
        pending,
        errors,
        ..
    } = kept;
    let diagrams = std::thread::scope(|scope| {
        // A large text is written while the pictures are read, so that neither pipe fills up.
        let writing = scope.spawn(|| {
            stdin.write_all(input.as_bytes())?;
            stdin.flush()
        });
        let diagrams = read_pictures(stdout, pending, &marker);
        let written = writing.join().ok().and_then(Result::ok);
        diagrams.filter(|_| written.is_some())
    })?;
    let errors = drawing_errors(errors, &marker);
    // Java's warnings, written when it starts, are no mistake; PlantUML's own and a crash are.
    let found = mistakes(&errors, text, &diagrams);
    let mistaken = errors.lines().any(|line| line.trim() == "ERROR")
        || found.iter().any(|mistake| mistake.details.is_some());
    if diagrams.is_empty() {
        let message = if mistaken {
            errors.trim().to_owned()
        } else {
            String::new()
        };
        return Some(Err(NotDrawn::Failed(if message.is_empty() {
            t!("diagrams.plantuml-no-diagram")
        } else {
            message
        })));
    }
    Some(Ok(Drawn {
        diagrams,
        mistakes: if mistaken { found } else { Vec::new() },
    }))
}

/// The pictures PlantUML writes to `stdout` up to the one that shows `marker`, the end of the
/// drawing; what comes after it stays in `pending`. `None` when the output ends first.
fn read_pictures(
    stdout: &mut ChildStdout,
    pending: &mut Vec<u8>,
    marker: &str,
) -> Option<Vec<Vec<u8>>> {
    let mut diagrams = Vec::new();
    let mut buffer = std::mem::take(pending);
    let mut chunk = [0; 64 * 1024];
    loop {
        while let Some(at) = find(&buffer, DIAGRAM_END.as_bytes()) {
            let part: Vec<u8> = buffer.drain(..at + DIAGRAM_END.len()).take(at).collect();
            if String::from_utf8_lossy(&part).contains(marker) {
                *pending = buffer;
                return Some(diagrams);
            }
            diagrams.extend(split_diagrams(&part));
        }
        match stdout.read(&mut chunk) {
            Ok(0) | Err(_) => return None,
            Ok(read) => buffer.extend_from_slice(&chunk[..read]),
        }
    }
}

/// The error output of the drawing that ends with `marker`: the lines before the end's own
/// error (`ERROR`, its line and its message), which are taken off with them.
fn drawing_errors(errors: &Errors, marker: &str) -> String {
    let mut state = errors
        .lines
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let ends = |lines: &[String]| lines.iter().position(|line| line.contains(marker));
    let mut waited = Duration::ZERO;
    while ends(&state.0).is_none() && !state.1 && waited < ERRORS_WAIT {
        let step = Duration::from_millis(50);
        state = errors
            .added
            .wait_timeout(state, step)
            .unwrap_or_else(|error| error.into_inner())
            .0;
        waited += step;
    }
    let Some(end) = ends(&state.0) else {
        return std::mem::take(&mut state.0).join("\n");
    };
    let lines: Vec<String> = state.0.drain(..=end).collect();
    // `ERROR` and the line number before the end's message.
    let own = lines.len().saturating_sub(3);
    lines[..own].join("\n")
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Ends every PlantUML kept running (for tests, and when the settings that run it change).
pub fn stop_all() {
    KEPT.lock()
        .unwrap_or_else(|error| error.into_inner())
        .clear();
}

/// The folders of the PlantUMLs kept running, the one used last at the end (for tests).
pub fn kept_folders() -> Vec<Option<PathBuf>> {
    KEPT.lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .map(|kept| kept.key.folder.clone())
        .collect()
}
