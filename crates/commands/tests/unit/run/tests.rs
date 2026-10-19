use super::*;

fn vars() -> RunVariables {
    RunVariables {
        full_current_path: r"C:\work\notes.final.txt".into(),
        file_name: "notes.final.txt".into(),
        current_directory: r"C:\work".into(),
        current_word: "birch".into(),
        current_line: 12,
        current_column: 3,
        current_line_text: "a birch tree".into(),
        executable: r"C:\Birchpad\birchpad.exe".into(),
        executable_directory: r"C:\Birchpad".into(),
    }
}

fn expand(line: &str) -> String {
    expand_variables(line, &vars(), |name| {
        (name == "HOME").then(|| "/home/me".into())
    })
}

#[test]
fn every_variable() {
    assert_eq!(expand("$(FULL_CURRENT_PATH)"), r"C:\work\notes.final.txt");
    assert_eq!(
        expand("$(CURRENT_DIRECTORY)|$(FILE_NAME)"),
        r"C:\work|notes.final.txt"
    );
    assert_eq!(expand("$(NAME_PART)$(EXT_PART)"), "notes.final.txt");
    assert_eq!(expand("[$(NAME_PART)][$(EXT_PART)]"), "[notes.final][.txt]");
    assert_eq!(
        expand("$(CURRENT_WORD) $(CURRENT_LINE):$(CURRENT_COLUMN)"),
        "birch 12:3"
    );
    assert_eq!(expand("$(CURRENT_LINESTR)"), "a birch tree");
    assert_eq!(
        expand("$(NPP_DIRECTORY) $(NPP_FULL_FILE_PATH)"),
        r"C:\Birchpad C:\Birchpad\birchpad.exe"
    );
    assert_eq!(expand("$(SYS.HOME)/x $(SYS.NOPE)."), "/home/me/x .");
    assert_eq!(expand("$(current_word)"), "birch");
}

#[test]
fn what_is_not_a_variable_stays() {
    for line in [
        "",
        "plain",
        "$(UNKNOWN)",
        "$(",
        "$(CURRENT_WORD",
        "$",
        "($CURRENT_WORD)",
        "$()",
        "a $(x) b",
    ] {
        assert_eq!(expand(line), line, "{line:?}");
    }
    assert_eq!(expand("$(UNKNOWN)$(CURRENT_WORD)"), "$(UNKNOWN)birch");
    assert_eq!(expand("$($(CURRENT_WORD))"), "$($(CURRENT_WORD))");
    // Without an extension, or a name that starts with a dot.
    let mut vars = vars();
    vars.file_name = "Makefile".into();
    assert_eq!(
        expand_variables("$(NAME_PART)|$(EXT_PART)", &vars, |_| None),
        "Makefile|"
    );
    vars.file_name = ".bashrc".into();
    assert_eq!(
        expand_variables("$(NAME_PART)|$(EXT_PART)", &vars, |_| None),
        ".bashrc|"
    );
    vars.file_name = "д.тxt".into();
    assert_eq!(expand_variables("$(EXT_PART)", &vars, |_| None), ".тxt");
}

#[test]
fn run_commands_round_trip() {
    assert!(
        default_run_commands().is_empty(),
        "the built-in ones are not saved"
    );
    let commands = builtin_run_commands();
    assert_eq!(commands.len(), 4);
    assert_eq!(
        parse_run_commands(&write_run_commands(&commands)).unwrap(),
        commands
    );
    assert_eq!(parse_run_commands("").unwrap(), vec![]);
    for bad in [
        "[[command]",
        "[[command]]\nline = 'x'\n",
        "[[command]]\nname = ''\nline = 'x'\n",
        "[[command]]\nname = 'a'\n",
        "[[command]]\nname = 'a'\nline = 'x'\n[[command]]\nname = 'a'\nline = 'y'\n",
    ] {
        assert!(parse_run_commands(bad).is_err(), "{bad}");
    }
}

#[test]
fn urls_and_shell_lines() {
    for line in ["https://example.com", "  HTTP://x", "https://", "http://"] {
        assert!(is_url(line), "{line}");
    }
    for line in [
        "",
        "   ",
        "notepad.exe",
        "ftp://x",
        "start https://x",
        "http:/x",
        "https:",
    ] {
        assert!(!is_url(line), "{line}");
    }
    let command = shell_command("echo hi");
    let args: Vec<_> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    if cfg!(windows) {
        assert_eq!(command.get_program(), "cmd");
        assert_eq!(args, ["/C", "echo hi"]);
    } else {
        assert_eq!(command.get_program(), "sh");
        assert_eq!(args, ["-c", "echo hi"]);
    }
}

#[test]
fn a_line_runs_as_typed() {
    // Quotes and the shell's own characters reach the shell untouched.
    let line = r#"echo "a b" & echo 'c' | more"#;
    let command = shell_command(line);
    let last = command
        .get_args()
        .last()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert_eq!(last, line);
    assert_eq!(shell_command("").get_args().count(), 2, "an empty line too");
}
