use std::path::PathBuf;

use birchpad_cli::explorer::*;
use std::path::Path;

#[test]
fn the_command_quotes_the_program_and_the_file() {
    let exe = PathBuf::from(r"C:\Program Files\Birchpad\birchpad.exe");
    assert_eq!(
        command_line(&exe),
        r#""C:\Program Files\Birchpad\birchpad.exe" "%1""#
    );
    assert_eq!(command_line(Path::new("")), r#""" "%1""#);
}

/// In a key of the tests' own, not the Explorer's.
#[cfg(windows)]
#[test]
fn added_changed_and_taken_away() {
    use windows_registry::CURRENT_USER;
    let root = format!(r"Software\Birchpad-tests\explorer-{}", std::process::id());
    let path = format!(r"{root}\Classes\*\shell\Birchpad");
    assert!(!is_added_at(&path));
    remove_at(&path).unwrap();

    let first = PathBuf::from(r"C:\Birchpad\birchpad.exe");
    add_at(&path, &first).unwrap();
    assert!(is_added_at(&path));
    let key = CURRENT_USER.open(&path).unwrap();
    assert_eq!(key.get_string("").unwrap(), LABEL);
    assert_eq!(
        key.get_string("Icon").unwrap(),
        r#""C:\Birchpad\birchpad.exe",0"#
    );
    // Added again from another copy: it runs that one.
    let second = PathBuf::from(r"D:\portable birchpad\birchpad.exe");
    add_at(&path, &second).unwrap();
    let command = CURRENT_USER.open(format!(r"{path}\command")).unwrap();
    assert_eq!(command.get_string("").unwrap(), command_line(&second));

    remove_at(&path).unwrap();
    assert!(!is_added_at(&path));
    remove_at(&path).unwrap();
    CURRENT_USER.remove_tree(&root).unwrap();
}

/// Taken away by the user: updates leave it away, until the user adds it again; uninstalling
/// forgets it.
#[cfg(windows)]
#[test]
fn a_verb_taken_away_stays_away_through_updates() {
    use windows_registry::CURRENT_USER;
    let state = format!(
        r"Software\Birchpad-tests\explorer-state-{}",
        std::process::id()
    );
    assert!(!was_removed(&state), "nothing remembered yet");
    remember_removed(&state, false).unwrap();
    assert!(!was_removed(&state), "forgetting nothing is no error");
    remember_removed(&state, true).unwrap();
    assert!(was_removed(&state));
    remember_removed(&state, true).unwrap();
    assert!(was_removed(&state), "twice is once");
    remember_removed(&state, false).unwrap();
    assert!(!was_removed(&state));
    // Another value there is not the user's choice.
    CURRENT_USER
        .create(&state)
        .unwrap()
        .set_string(REMOVED, "kept")
        .unwrap();
    assert!(!was_removed(&state));
    CURRENT_USER.remove_tree(&state).unwrap();
}

#[cfg(not(windows))]
#[test]
fn elsewhere_there_is_no_such_menu() {
    assert!(add(Path::new("/usr/bin/birchpad")).is_err());
    remove().unwrap();
    assert!(!is_added());
}
