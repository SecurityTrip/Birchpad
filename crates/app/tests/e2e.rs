//! End-to-end tests of the built application: a portable copy in a folder of its own starts in a
//! real window, follows a script (src/e2e.rs) and quits; the test checks what its reports say
//! and what is on disk, then starts it again. Only with the `e2e` feature:
//!
//! ```text
//! cargo test -p birchpad --features e2e --test e2e
//! ```
//!
//! On Linux they need a display (`xvfb-run`).
#![cfg(feature = "e2e")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

/// How long a run may take: start, the script, quit.
const TIMEOUT: Duration = Duration::from_secs(90);

/// A portable copy of the application in a temporary folder: its settings, sessions and
/// backups stay there.
struct Install {
    dir: tempfile::TempDir,
    exe: PathBuf,
}

impl Install {
    fn new() -> Self {
        Self::with_settings("")
    }

    /// With more settings (TOML) after the ones every run has.
    fn with_settings(extra: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let built = Path::new(env!("CARGO_BIN_EXE_birchpad"));
        let exe = dir.path().join(built.file_name().unwrap());
        fs::copy(built, &exe).unwrap();
        fs::write(dir.path().join(birchpad_config::PORTABLE_MARKER), "").unwrap();
        let data = dir.path().join(birchpad_config::PORTABLE_DATA_DIR);
        fs::create_dir_all(&data).unwrap();
        // No update checks from tests, and no offer to open text files, whose question would
        // take the focus from the scripts' typing; messages in English, whatever the system's
        // language.
        fs::write(
            data.join("settings.toml"),
            format!(
                "[updates]\nmode = \"off\"\n\n[files]\noffer-default-app = false\n\n\
                 [appearance]\nlanguage = \"en\"\n\n{extra}"
            ),
        )
        .unwrap();
        Self { dir, exe }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// Runs the application with `args` and `script` (TOML steps), until it quits; returns its
    /// reports by name.
    fn run(&self, args: &[&str], script: &str) -> Reports {
        self.spawn(args, script).wait()
    }

    /// Starts the application with `args` and `script`, without waiting for it.
    fn spawn(&self, args: &[&str], script: &str) -> Running {
        let script_path = self.path("script.toml");
        fs::write(&script_path, script).unwrap();
        for entry in fs::read_dir(self.dir.path()).unwrap() {
            let path = entry.unwrap().path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                fs::remove_file(path).unwrap();
            }
        }
        let log = self.path("stderr.txt");
        let child = Command::new(&self.exe)
            .args(args)
            .current_dir(self.dir.path())
            .env("BIRCHPAD_E2E_SCRIPT", &script_path)
            .stdout(Stdio::null())
            .stderr(fs::File::create(&log).unwrap())
            .spawn()
            .unwrap();
        Running {
            child,
            log,
            dir: self.dir.path().to_owned(),
            started: Instant::now(),
        }
    }

    /// The path of `name` in the folder, as a literal TOML string.
    fn literal(&self, name: &str) -> String {
        format!("'{}'", self.path(name).display())
    }
}

/// A run that has started.
struct Running {
    child: Child,
    log: PathBuf,
    dir: PathBuf,
    started: Instant,
}

impl Running {
    /// Waits until it quits; its reports by name.
    fn wait(mut self) -> Reports {
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            if self.started.elapsed() > TIMEOUT {
                self.kill();
                panic!(
                    "still running after {TIMEOUT:?}; stderr:\n{}",
                    self.stderr()
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        let stderr = self.stderr();
        assert!(status.success(), "exited with {status}; stderr:\n{stderr}");
        self.reports()
    }

    /// Waits until the report `name` is written.
    fn wait_for(&mut self, name: &str) -> Reports {
        let path = self.dir.join(name);
        while !path.exists() {
            if let Some(status) = self.child.try_wait().unwrap() {
                panic!(
                    "quit with {status} before {name}; stderr:\n{}",
                    self.stderr()
                );
            }
            if self.started.elapsed() > TIMEOUT {
                self.kill();
                panic!("no {name} after {TIMEOUT:?}; stderr:\n{}", self.stderr());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        // Written whole before it is read.
        std::thread::sleep(Duration::from_millis(200));
        self.reports()
    }

    /// Ends it as a crash would: nothing is saved.
    fn kill(&mut self) {
        self.child.kill().ok();
        self.child.wait().ok();
    }

    fn stderr(&self) -> String {
        fs::read_to_string(&self.log).unwrap_or_default()
    }

    fn reports(&self) -> Reports {
        Reports {
            dir: self.dir.clone(),
            stderr: self.stderr(),
        }
    }
}

struct Reports {
    dir: PathBuf,
    stderr: String,
}

impl Reports {
    /// The report written as `name`, which must list no failed step.
    fn get(&self, name: &str) -> Value {
        let path = self.dir.join(name);
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("no report {name} ({error}); stderr:\n{}", self.stderr));
        let report: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(report["errors"], Value::Array(Vec::new()), "{name}");
        report
    }

    /// The report written as `name`, with the steps that failed before it.
    fn with_errors(&self, name: &str) -> (Value, Vec<String>) {
        let text = fs::read_to_string(self.dir.join(name)).unwrap();
        let report: Value = serde_json::from_str(&text).unwrap();
        let errors = report["errors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|error| error.as_str().unwrap().to_owned())
            .collect();
        (report, errors)
    }
}

/// The tabs of a report as (name, text, modified).
fn tabs(report: &Value) -> Vec<(String, String, bool)> {
    report["tabs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tab| {
            (
                tab["name"].as_str().unwrap().to_owned(),
                tab["text"].as_str().unwrap().to_owned(),
                tab["modified"].as_bool().unwrap(),
            )
        })
        .collect()
}

/// What `text = "not saved\n\tindented"` types into a new document, whose line breaks are the
/// platform's (CRLF on Windows, as in Notepad++).
fn untitled_text() -> String {
    let newline = if cfg!(windows) { "\r\n" } else { "\n" };
    format!("not saved{newline}\tindented")
}

fn tab(name: &str, text: &str, modified: bool) -> (String, String, bool) {
    (name.to_owned(), text.to_owned(), modified)
}

#[test]
fn edits_are_saved_and_the_session_comes_back() {
    let install = Install::new();
    fs::write(install.path("notes.txt"), "first line\n").unwrap();

    let first = install.run(
        &["notes.txt"],
        r#"
        [[step]]
        report = "opened.json"
        [[step]]
        command = "cursor.document-end"
        [[step]]
        text = "second line"
        [[step]]
        report = "typed.json"
        [[step]]
        command = "file.save"
        [[step]]
        report = "saved.json"
        [[step]]
        command = "file.new"
        [[step]]
        text = "not saved\n\tindented"
        [[step]]
        report = "untitled.json"
        [[step]]
        command = "file.exit"
        "#,
    );
    // The file replaces the empty "new 1" the window starts with.
    let opened = first.get("opened.json");
    assert_eq!(tabs(&opened), [tab("notes.txt", "first line\n", false)]);
    assert_eq!(opened["tabs"][0]["caret"], 0);
    let typed = first.get("typed.json");
    assert_eq!(
        tabs(&typed),
        [tab("notes.txt", "first line\nsecond line", true)]
    );
    assert_eq!(typed["tabs"][0]["caret"], 22);
    let saved = first.get("saved.json");
    assert_eq!(
        tabs(&saved),
        [tab("notes.txt", "first line\nsecond line", false)]
    );
    assert_eq!(
        fs::read_to_string(install.path("notes.txt")).unwrap(),
        "first line\nsecond line"
    );
    let untitled = first.get("untitled.json");
    assert_eq!(tabs(&untitled)[1], tab("new 1", &untitled_text(), true));
    assert_eq!(untitled["active"], 1);

    // Quitting did not ask about "new 1": the session keeps it, unsaved, with the caret and the
    // active tab.
    let second = install.run(
        &[],
        r#"
        [[step]]
        report = "restored.json"
        [[step]]
        command = "file.exit"
        "#,
    );
    let restored = second.get("restored.json");
    assert_eq!(
        tabs(&restored),
        [
            tab("notes.txt", "first line\nsecond line", false),
            tab("new 1", &untitled_text(), true),
        ]
    );
    assert_eq!(restored["active"], 1);
    assert_eq!(restored["tabs"][0]["caret"], 22);
    assert_eq!(restored["tabs"][1]["caret"], untitled_text().len());
}

#[test]
fn nosession_starts_empty_and_leaves_the_session_alone() {
    let install = Install::new();
    fs::write(install.path("a.txt"), "alpha").unwrap();
    let exit = r#"
        [[step]]
        report = "report.json"
        [[step]]
        command = "file.exit"
        "#;
    let first = install.run(&["a.txt"], exit);
    assert_eq!(
        tabs(&first.get("report.json")),
        [tab("a.txt", "alpha", false)]
    );

    let without = install.run(&["-nosession"], exit);
    assert_eq!(tabs(&without.get("report.json")), [tab("new 1", "", false)]);

    // -nosession neither restored nor replaced the session.
    let after = install.run(&[], exit);
    assert_eq!(
        tabs(&after.get("report.json")),
        [tab("a.txt", "alpha", false)]
    );
}

#[test]
fn a_read_only_file_and_a_failed_step_change_nothing() {
    let install = Install::new();
    fs::write(install.path("locked.txt"), "keep\n").unwrap();
    let run = install.run(
        &["-ro", "locked.txt"],
        r#"
        [[step]]
        keys = "down end"
        [[step]]
        text = "x"
        [[step]]
        command = "file.save"
        [[step]]
        command = "no.such-command"
        [[step]]
        report = "report.json"
        [[step]]
        command = "file.exit"
        "#,
    );
    let (report, errors) = run.with_errors("report.json");
    assert_eq!(tabs(&report), [tab("locked.txt", "keep\n", false)]);
    assert!(report["tabs"][0]["read_only"].is_string());
    // Typing and saving are no failures: the document ignores them.
    assert_eq!(errors, ["step 4: unknown command no.such-command"]);
    assert_eq!(fs::read(install.path("locked.txt")).unwrap(), b"keep\n");
}

#[test]
fn a_utf16_file_with_crlf_saves_in_its_own_encoding() {
    let install = Install::new();
    let utf16 = |text: &str| {
        let mut bytes = vec![0xff, 0xfe];
        bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        bytes
    };
    fs::write(install.path("wide.txt"), utf16("é\r\nb")).unwrap();
    let run = install.run(
        &["wide.txt"],
        r#"
        [[step]]
        keys = "down end"
        [[step]]
        text = "c\nd"
        [[step]]
        command = "file.save"
        [[step]]
        report = "report.json"
        [[step]]
        command = "file.exit"
        "#,
    );
    assert_eq!(
        tabs(&run.get("report.json")),
        [tab("wide.txt", "é\r\nbc\r\nd", false)]
    );
    assert_eq!(
        fs::read(install.path("wide.txt")).unwrap(),
        utf16("é\r\nbc\r\nd")
    );
}

/// The rows of a report's search results.
fn results(report: &Value) -> Vec<String> {
    report["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn find_and_replace_in_files() {
    let install = Install::new();
    let src = install.path("src");
    fs::create_dir(&src).unwrap();
    fs::write(src.join("a.txt"), "red apple\nred\n").unwrap();
    fs::write(src.join("b.md"), "red").unwrap();
    fs::write(src.join("c.txt"), "blue").unwrap();
    let run = install.run(
        &[],
        r#"
        [[step]]
        files = { pattern = "red", folder = "src", filters = "*.txt" }
        [[step]]
        report = "found.json"
        [[step]]
        files = { pattern = "red", folder = "src", filters = "*.txt", replace = "green" }
        [[step]]
        report = "replaced.json"
        [[step]]
        command = "file.exit"
        "#,
    );
    let found = results(&run.get("found.json"));
    assert!(
        found
            .iter()
            .any(|row| row.contains("a.txt") && row.contains("(2)")),
        "{found:?}"
    );
    assert!(
        !found.iter().any(|row| row.contains("b.md")),
        "filtered out"
    );
    assert!(!found.iter().any(|row| row.contains("c.txt")), "no hit");
    run.get("replaced.json");
    assert_eq!(
        fs::read_to_string(src.join("a.txt")).unwrap(),
        "green apple\ngreen\n"
    );
    assert_eq!(fs::read_to_string(src.join("b.md")).unwrap(), "red");
    assert_eq!(fs::read_to_string(src.join("c.txt")).unwrap(), "blue");
}

#[test]
fn a_recorded_macro_plays_back() {
    let install = Install::new();
    let run = install.run(
        &[],
        r#"
        [[step]]
        command = "macro.start-recording"
        [[step]]
        text = "ab"
        [[step]]
        command = "macro.stop-recording"
        [[step]]
        command = "macro.playback"
        [[step]]
        command = "macro.playback"
        [[step]]
        report = "played.json"
        [[step]]
        command = "file.exit"
        "#,
    );
    let played = run.get("played.json");
    assert_eq!(tabs(&played), [tab("new 1", "ababab", true)]);
    assert_eq!(played["tabs"][0]["caret"], 6);
}

#[test]
fn a_second_launch_hands_its_files_to_the_first() {
    let install = Install::new();
    fs::write(install.path("a.txt"), "alpha").unwrap();
    fs::write(install.path("b.txt"), "beta").unwrap();
    let mut first = install.spawn(
        &["a.txt"],
        r#"
        [[step]]
        report = "ready.json"
        [[step]]
        wait = 4000
        [[step]]
        report = "handed.json"
        [[step]]
        command = "file.exit"
        "#,
    );
    first.wait_for("ready.json");
    // The second launch hands b.txt over and quits, without a window of its own.
    let second = Command::new(&install.exe)
        .arg("b.txt")
        .current_dir(install.dir.path())
        .status()
        .unwrap();
    assert!(second.success());
    let reports = first.wait();
    assert_eq!(
        tabs(&reports.get("handed.json")),
        [tab("a.txt", "alpha", false), tab("b.txt", "beta", false)]
    );
}

#[test]
fn unsaved_text_comes_back_after_the_process_is_killed() {
    let install = Install::with_settings("[session]\nbackup-interval-seconds = 1\n");
    let mut first = install.spawn(
        &[],
        r#"
        [[step]]
        text = "draft before the crash"
        [[step]]
        wait = 3000
        [[step]]
        report = "typed.json"
        [[step]]
        wait = 60000
        "#,
    );
    let typed = first.wait_for("typed.json");
    assert_eq!(
        tabs(&typed.get("typed.json")),
        [tab("new 1", "draft before the crash", true)]
    );
    first.kill();
    let second = install.run(
        &[],
        r#"
        [[step]]
        report = "restored.json"
        [[step]]
        command = "file.exit"
        "#,
    );
    assert_eq!(
        tabs(&second.get("restored.json")),
        [tab("new 1", "draft before the crash", true)]
    );
}

#[test]
fn a_plugin_is_installed_and_run() {
    let install = Install::new();
    let folder = install.path("downloads").join("probe");
    fs::create_dir_all(&folder).unwrap();
    fs::write(
        folder.join("plugin.toml"),
        "id = \"probe\"\nname = \"Probe\"\nversion = \"1.0\"\napi = \"0.1\"\n",
    )
    .unwrap();
    let wasm = Path::new(env!("CARGO_MANIFEST_DIR")).join("../plugin/tests/fixtures/probe.wasm");
    fs::copy(wasm, folder.join("plugin.wasm")).unwrap();
    let manifest = install.literal("downloads/probe/plugin.toml");
    let run = install.run(
        &[],
        &format!(
            r#"
            [[step]]
            command = "plugin.install"
            args = {{ path = {manifest} }}
            [[step]]
            wait = 3000
            [[step]]
            text = "one two"
            [[step]]
            command = "edit.select-all"
            [[step]]
            command = "plugin.run"
            args = {{ plugin = "probe", command = "upper" }}
            [[step]]
            report = "ran.json"
            [[step]]
            command = "file.exit"
            "#
        ),
    );
    assert_eq!(tabs(&run.get("ran.json")), [tab("new 1", "ONE TWO", true)]);
}

#[test]
fn a_document_is_exported_to_html() {
    let install = Install::new();
    fs::write(install.path("main.rs"), "fn main() {}\n").unwrap();
    let out = install.literal("main.html");
    let run = install.run(
        &["main.rs"],
        &format!(
            r#"
            [[step]]
            command = "file.export-html"
            args = {{ path = {out} }}
            [[step]]
            report = "exported.json"
            [[step]]
            command = "file.exit"
            "#
        ),
    );
    run.get("exported.json");
    let html = fs::read_to_string(install.path("main.html")).unwrap();
    assert!(html.contains("<html"), "{html}");
    assert!(html.contains("main"), "{html}");
}
