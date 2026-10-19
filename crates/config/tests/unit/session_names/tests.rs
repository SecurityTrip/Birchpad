use super::*;

#[test]
fn line_endings_by_name() {
    for line_ending in [LineEnding::CrLf, LineEnding::Lf, LineEnding::Cr] {
        assert_eq!(
            parse_line_ending(line_ending_name(line_ending)),
            Some(line_ending)
        );
    }
    for unknown in ["", "CRLF", "lfcr", "unix"] {
        assert_eq!(parse_line_ending(unknown), None, "{unknown}");
    }
}

#[test]
fn languages_by_name() {
    assert_eq!(LanguageName::parse("text"), LanguageName::Text);
    assert_eq!(
        LanguageName::parse("udl:Markdown"),
        LanguageName::User("Markdown")
    );
    assert_eq!(
        LanguageName::parse("udl:"),
        LanguageName::User(""),
        "an empty name"
    );
    assert_eq!(LanguageName::parse("rust"), LanguageName::Builtin("rust"));
    assert_eq!(LanguageName::parse("Text"), LanguageName::Builtin("Text"));
    assert_eq!(LanguageName::parse(""), LanguageName::Builtin(""));
    for name in ["text", "udl:My lang", "python"] {
        assert_eq!(LanguageName::parse(name).write(), name);
    }
}

#[test]
fn backup_names_are_file_names_and_unique() {
    let first = backup_name(r#"a\b/c:d*e?f"g<h>i|j"#);
    assert!(first.starts_with("a_b_c_d_e_f_g_h_i_j@"), "{first}");
    assert_ne!(backup_name("new 1"), backup_name("new 1"));
    assert!(backup_name("").starts_with('@'), "an empty name");
    assert!(backup_name("заметки").starts_with("заметки@"));
}

#[test]
fn untitled_numbers_are_kept_or_the_lowest_free() {
    let mut numbers = HashSet::new();
    assert_eq!(free_untitled_number(Some(3), &mut numbers), 3);
    assert_eq!(free_untitled_number(Some(3), &mut numbers), 1, "taken");
    assert_eq!(free_untitled_number(None, &mut numbers), 2);
    assert_eq!(
        free_untitled_number(Some(0), &mut numbers),
        4,
        "0 is not a number"
    );
    assert_eq!(
        free_untitled_number(Some(usize::MAX), &mut numbers),
        usize::MAX
    );
}
