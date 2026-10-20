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

#[test]
fn every_listed_option_is_understood() {
    let cwd = std::path::Path::new("/work");
    let mut seen = std::collections::HashSet::new();
    for option in OPTIONS {
        assert!(seen.insert(option.syntax), "{} twice", option.syntax);
        assert!(!option.description.is_empty());
        // The syntax with its placeholder filled in parses without a warning.
        let written = option
            .syntax
            .replace("<line>", "3")
            .replace("<column>", "4")
            .replace("<position>", "5")
            .replace("<language>", "cpp")
            .replace("<name>", "Markdown")
            .replace("<left>", "10")
            .replace("<top>", "20")
            .replace("<text>", "hello")
            .replace("<file>", "quote.txt")
            .replace("<folder>", "cfg")
            .replace("<1-3>", "2")
            .replace("file...", "a.txt");
        assert!(
            !written.contains('<'),
            "{written} has its placeholder filled in"
        );
        let parsed = CommandLine::parse([std::ffi::OsString::from(written.clone())], cwd);
        assert!(
            parsed.warnings.is_empty(),
            "{written}: {:?}",
            parsed.warnings
        );
    }
}

#[test]
fn notepad_plus_plus_window_and_file_options() {
    let line = parse(&[
        "-notabbar",
        "-alwaysOnTop",
        "-systemtray",
        "-loadingTime",
        "-quickPrint",
        "-monitor",
        "-openFoldersAsWorkspace",
        "-nopluginsupport",
        "-x100",
        "-y-20",
        "-titleAdd=\"My work\"",
        "-settingsDir=cfg",
        "-udl=Markdown (preinstalled)",
        "-pluginMessage=hello=world",
    ]);
    assert!(line.warnings.is_empty(), "{:?}", line.warnings);
    assert!(line.no_tab_bar && line.always_on_top && line.minimized && line.loading_time);
    assert!(line.quick_print && line.monitor && line.open_folders_as_workspace);
    assert!(line.no_plugins);
    assert_eq!((line.x, line.y), (Some(100), Some(-20)));
    assert_eq!(line.title_add.as_deref(), Some("My work"));
    assert_eq!(line.settings_dir, Some(PathBuf::from("/work/cfg")));
    assert_eq!(line.udl.as_deref(), Some("Markdown (preinstalled)"));
    assert_eq!(line.plugin_message.as_deref(), Some("hello=world"));
    // Names in any case, values as written.
    let upper = parse(&["-NOTABBAR", "-TITLEADD=Mixed Case"]);
    assert!(upper.no_tab_bar);
    assert_eq!(upper.title_add.as_deref(), Some("Mixed Case"));
}

#[test]
fn ghost_typing_options() {
    let line = parse(&["-qt=\"Hello\"", "-qSpeed3"]);
    let ghost = line.ghost.unwrap();
    assert_eq!(ghost.text.as_deref(), Some("Hello"));
    assert_eq!(ghost.speed, 3);
    let file = parse(&["-qf=quote.txt"]).ghost.unwrap();
    assert_eq!(file.file, Some(PathBuf::from("/work/quote.txt")));
    assert_eq!(file.speed, 2, "fast by default");
    assert_eq!(
        parse(&["-qn=Joel"]).ghost.unwrap().name.as_deref(),
        Some("Joel")
    );
    assert_eq!(parse(&["-qSpeed1"]).ghost.unwrap().speed, 1);
    assert!(parse(&["a.txt"]).ghost.is_none());
}

#[test]
fn wrong_values_warn() {
    for args in [
        &["-qSpeed0"][..],
        &["-qSpeed4"],
        &["-qSpeed"],
        &["-xleft"],
        &["-y"],
        &["-settingsDir="],
    ] {
        let line = parse(args);
        assert_eq!(line.warnings.len(), 1, "{args:?}: {:?}", line.warnings);
    }
    // Empty values elsewhere are values.
    assert_eq!(parse(&["-titleAdd="]).title_add.as_deref(), Some(""));
    assert_eq!(parse(&["-udl=\"\""]).udl.as_deref(), Some(""));
}

#[test]
fn wildcards_match_names() {
    for (pattern, name) in [
        ("*", ""),
        ("*", "a.txt"),
        ("*.txt", "a.txt"),
        ("*.txt", ".txt"),
        ("a?c", "abc"),
        ("*b*", "abc"),
        ("a*c*e", "abcde"),
        ("**", "x"),
        ("", ""),
    ] {
        assert!(wildcard_match(pattern, name), "{pattern} {name}");
    }
    for (pattern, name) in [
        ("*.txt", "a.md"),
        ("a?c", "ac"),
        ("a?c", "abbc"),
        ("", "a"),
        ("a", ""),
        ("*.txt", "a.txt.bak"),
    ] {
        assert!(!wildcard_match(pattern, name), "{pattern} {name}");
    }
    // Windows' file names have no case.
    assert_eq!(wildcard_match("*.TXT", "a.txt"), cfg!(windows));
}

#[test]
fn wildcards_expand_in_their_folder_and_below_with_r() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("b.txt"), "").unwrap();
    std::fs::write(dir.path().join("a.txt"), "").unwrap();
    std::fs::write(dir.path().join("c.md"), "").unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    std::fs::write(dir.path().join("sub").join("d.txt"), "").unwrap();
    let parse_in = |args: &[&str]| CommandLine::parse(args.iter().map(OsString::from), dir.path());
    let names = |line: &CommandLine| -> Vec<String> {
        line.files
            .iter()
            .map(|path| {
                path.strip_prefix(dir.path())
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect()
    };
    assert_eq!(names(&parse_in(&["*.txt"])), ["a.txt", "b.txt"], "sorted");
    assert_eq!(
        names(&parse_in(&["*.txt", "-r"])),
        ["a.txt", "b.txt", "sub/d.txt"],
        "-r anywhere on the line"
    );
    assert_eq!(names(&parse_in(&["sub/*"])), ["sub/d.txt"]);
    let none = parse_in(&["*.rs"]);
    assert!(none.files.is_empty());
    assert_eq!(none.warnings.len(), 1, "nothing matches");
    // A name without wildcards stays, whether or not it exists.
    assert_eq!(names(&parse_in(&["new.txt"])), ["new.txt"]);
}

#[test]
fn parsing_unexpanded_leaves_the_file_system_alone() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "").unwrap();
    let args = || ["*.txt", "-r", "/*", "b?.md", "plain.txt"].map(OsString::from);
    let line = CommandLine::parse_unexpanded(args(), dir.path());
    // The wildcards stay as written, even `/*` with `-r`, which would walk the whole disk.
    assert_eq!(
        line.files,
        [
            dir.path().join("*.txt"),
            dir.path().join("/*"),
            dir.path().join("b?.md"),
            dir.path().join("plain.txt"),
        ]
    );
    assert!(line.recursive);
    assert!(line.warnings.is_empty(), "nothing was matched to warn of");
    // Otherwise the same as parsing; nothing given, nothing parsed.
    let plain = ["-n3", "-ro", "a.txt"].map(OsString::from);
    assert_eq!(
        CommandLine::parse_unexpanded(plain.clone(), dir.path()),
        CommandLine::parse(plain, dir.path())
    );
    assert_eq!(
        CommandLine::parse_unexpanded([], dir.path()),
        CommandLine::default()
    );
}

#[test]
fn older_command_lines_read_with_the_new_options_off() {
    let line: CommandLine =
        serde_json::from_str(r#"{"files":["/a.txt"],"line":3,"read_only":true}"#).unwrap();
    assert_eq!(line.line, Some(3));
    assert!(line.read_only && !line.quick_print && line.ghost.is_none());
    let full = parse(&["-qt=x", "-x5", "-udl=U", "a.txt"]);
    let json = serde_json::to_string(&full).unwrap();
    assert_eq!(serde_json::from_str::<CommandLine>(&json).unwrap(), full);
}
