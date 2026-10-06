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
use std::process::{Command, Stdio};
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
        let dir = tempfile::tempdir().unwrap();
        let built = Path::new(env!("CARGO_BIN_EXE_birchpad"));
        let exe = dir.path().join(built.file_name().unwrap());
        fs::copy(built, &exe).unwrap();
        fs::write(dir.path().join(birchpad_config::PORTABLE_MARKER), "").unwrap();
        let data = dir.path().join(birchpad_config::PORTABLE_DATA_DIR);
        fs::create_dir_all(&data).unwrap();
        // No update checks from tests.
        fs::write(data.join("settings.toml"), "[updates]\nmode = \"off\"\n").unwrap();
        Self { dir, exe }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// Runs the application with `args` and `script` (TOML steps), until it quits; returns its
    /// reports by name.
    fn run(&self, args: &[&str], script: &str) -> Reports {
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
        let mut child = Command::new(&self.exe)
            .args(args)
            .current_dir(self.dir.path())
            .env("BIRCHPAD_E2E_SCRIPT", &script_path)
            .stdout(Stdio::null())
            .stderr(fs::File::create(&log).unwrap())
            .spawn()
            .unwrap();
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if started.elapsed() > TIMEOUT {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!(
                    "still running after {TIMEOUT:?}; stderr:\n{}",
                    fs::read_to_string(&log).unwrap_or_default()
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        let stderr = fs::read_to_string(&log).unwrap_or_default();
        assert!(status.success(), "exited with {status}; stderr:\n{stderr}");
        Reports {
            dir: self.dir.path().to_owned(),
            stderr,
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
        keys = "secondary-end"
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
        keys = "secondary-end"
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
        keys = "secondary-end"
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
