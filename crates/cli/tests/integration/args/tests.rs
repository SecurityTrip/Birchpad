use birchpad_cli::*;
use std::ffi::OsString;
use std::path::Path;
use std::path::PathBuf;

fn parse(args: &[&str]) -> CommandLine {
    CommandLine::parse(args.iter().map(OsString::from), Path::new("/work"))
}

#[test]
fn files_are_made_absolute() {
    let line = parse(&["a.txt", "/abs/b.txt", "sub dir/with spaces.txt"]);
    assert_eq!(
        line.files,
        [
            PathBuf::from("/work/a.txt"),
            PathBuf::from("/abs/b.txt"),
            PathBuf::from("/work/sub dir/with spaces.txt"),
        ]
    );
    assert!(line.warnings.is_empty());
}

#[test]
fn notepad_plus_plus_options() {
    let line = parse(&[
        "-n120",
        "-c5",
        "-multiInst",
        "-nosession",
        "-ro",
        "-lcpp",
        "f.txt",
    ]);
    assert_eq!(line.line, Some(120));
    assert_eq!(line.column, Some(5));
    assert!(line.multi_instance && line.no_session && line.read_only);
    assert_eq!(line.language.as_deref(), Some("cpp"));
    assert_eq!(line.files, [PathBuf::from("/work/f.txt")]);
    assert!(!line.open_session);
    let session = parse(&["-openSession", "work.xml"]);
    assert!(session.open_session && session.warnings.is_empty());
    assert_eq!(session.files, [PathBuf::from("/work/work.xml")]);
    assert_eq!(
        line.caret_target(),
        Some(CaretTarget::LineColumn {
            line: 120,
            column: 5
        })
    );
}

#[test]
fn options_are_case_insensitive_and_position_wins() {
    let line = parse(&["-MULTIINST", "-N7", "-p42"]);
    assert!(line.multi_instance);
    assert_eq!(line.caret_target(), Some(CaretTarget::Position(42)));
    assert_eq!(
        parse(&["-c3"]).caret_target(),
        Some(CaretTarget::LineColumn { line: 1, column: 3 })
    );
    assert_eq!(parse(&["x"]).caret_target(), None);
}

#[test]
fn unknown_and_malformed_options_warn() {
    let line = parse(&[
        "-nfoo",
        "-frobnicate",
        "-notabbar",
        "-x100",
        "-settingsDir=C:\\cfg",
    ]);
    assert_eq!(line.warnings.len(), 2, "{:?}", line.warnings);
    assert_eq!(line.line, None);
}

#[test]
fn double_dash_ends_options() {
    let line = parse(&["-ro", "--", "-n5", "-"]);
    assert!(line.read_only);
    assert_eq!(line.line, None);
    assert_eq!(
        line.files,
        [PathBuf::from("/work/-n5"), PathBuf::from("/work/-")]
    );
}

#[test]
fn generate_takes_an_optional_count() {
    assert_eq!(parse(&["--generate", "500"]).generate, Some(500));
    let line = parse(&["--generate", "file.txt"]);
    assert_eq!(line.generate, Some(1_000_000));
    assert_eq!(line.files.len(), 1);
}

#[test]
fn round_trips_through_json() {
    let line = parse(&["-n3", "-ro", "a.txt"]);
    let json = serde_json::to_string(&line).unwrap();
    assert_eq!(serde_json::from_str::<CommandLine>(&json).unwrap(), line);
}
