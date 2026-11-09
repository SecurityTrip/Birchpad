use birchpad_lsp::command_line::*;

fn parse(line: &str) -> (String, Vec<String>) {
    let command = CommandLine::parse(line).unwrap();
    (command.program, command.args)
}

#[test]
fn splits_a_command_line_into_words() {
    assert_eq!(parse("rust-analyzer"), ("rust-analyzer".into(), vec![]));
    assert_eq!(
        parse("  pylsp   -v --log-file x.log "),
        (
            "pylsp".into(),
            vec!["-v".into(), "--log-file".into(), "x.log".into()]
        )
    );
    assert_eq!(
        parse("\tclangd\t--background-index\n"),
        ("clangd".into(), vec!["--background-index".into()])
    );
}

#[test]
fn quotes_keep_spaces_and_backslashes_stay() {
    assert_eq!(
        parse(r#""C:\Program Files\LLVM\bin\clangd.exe" --log=error"#),
        (
            r"C:\Program Files\LLVM\bin\clangd.exe".into(),
            vec!["--log=error".into()]
        )
    );
    assert_eq!(
        parse("node 'my server.js' --stdio"),
        ("node".into(), vec!["my server.js".into(), "--stdio".into()])
    );
    // Quotes inside a word, the other kind of quote inside quotes, an escaped quote.
    assert_eq!(
        parse(r#"a b"c d"e 'say "hi"' \"x\""#),
        (
            "a".into(),
            vec!["bc de".into(), "say \"hi\"".into(), "\"x\"".into()]
        )
    );
    // An empty argument in quotes is kept.
    assert_eq!(
        parse("x \"\" ''"),
        ("x".into(), vec![String::new(), String::new()])
    );
    // A backslash at the end stays.
    assert_eq!(parse(r"x C:\dir\"), ("x".into(), vec![r"C:\dir\".into()]));
}

#[test]
fn refuses_empty_and_unclosed_command_lines() {
    for empty in ["", "   ", "\t\n", "\"\"", "'' --stdio"] {
        assert_eq!(
            CommandLine::parse(empty),
            Err(CommandLineError::Empty),
            "{empty:?}"
        );
    }
    assert_eq!(
        CommandLine::parse("server \"--x"),
        Err(CommandLineError::UnclosedQuote('"'))
    );
    assert_eq!(
        CommandLine::parse("'server"),
        Err(CommandLineError::UnclosedQuote('\''))
    );
    assert!(
        CommandLineError::UnclosedQuote('"')
            .to_string()
            .contains('"')
    );
    assert!(!CommandLineError::Empty.to_string().is_empty());
}
