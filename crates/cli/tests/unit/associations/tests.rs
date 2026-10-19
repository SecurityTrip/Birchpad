use std::path::PathBuf;

use super::*;

#[test]
fn what_counts_as_birchpad() {
    assert!(is_birchpad_choice("Birchpad.File"));
    assert!(is_birchpad_choice(r"Applications\Birchpad.exe"));
    for other in ["", "txtfile", "Applications\\notepad.exe", "Birchpad.File2"] {
        assert!(!is_birchpad_choice(other), "{other}");
    }
    assert!(is_birchpad_desktop(
        "io.github.securitytrip.birchpad.desktop\n"
    ));
    assert!(!is_birchpad_desktop(""));
    assert!(!is_birchpad_desktop("org.gnome.TextEditor.desktop"));
    assert_eq!(
        open_command(&PathBuf::from(r"C:\Apps\Birchpad\birchpad.exe")),
        r#""C:\Apps\Birchpad\birchpad.exe" "%1""#
    );
    // The two lists name text files only, once each.
    let mut seen = std::collections::HashSet::new();
    assert!(EXTENSIONS.iter().all(|extension| seen.insert(*extension)));
    assert!(EXTENSIONS.contains(&"txt") && EXTENSIONS.contains(&"json"));
    assert!(MIME_TYPES.contains(&"text/plain"));
    assert_eq!(settings_page().is_some(), cfg!(windows));
    assert_eq!(supported(), !cfg!(target_os = "macos"));
}

/// In a key of the tests' own, not the user's classes.
#[cfg(windows)]
#[test]
fn registered_and_taken_away() {
    use windows_registry::CURRENT_USER;
    let root = format!(
        r"Software\Birchpad-tests\associations-{}",
        std::process::id()
    );
    assert!(!imp::is_registered(&root));
    imp::unregister(&root).unwrap();

    let exe = PathBuf::from(r"C:\Apps\Birchpad\birchpad.exe");
    imp::register(&root, &exe).unwrap();
    imp::register(&root, &exe).unwrap();
    assert!(imp::is_registered(&root));
    let command = CURRENT_USER
        .open(format!(r"{root}\Classes\{PROG_ID}\shell\open\command"))
        .unwrap();
    assert_eq!(command.get_string("").unwrap(), open_command(&exe));
    let txt = CURRENT_USER
        .open(format!(r"{root}\Classes\.txt\OpenWithProgids"))
        .unwrap();
    assert_eq!(txt.get_string(PROG_ID).unwrap(), "");
    let apps = CURRENT_USER
        .open(format!(r"{root}\RegisteredApplications"))
        .unwrap();
    assert_eq!(
        apps.get_string(APP_NAME).unwrap(),
        r"Software\Birchpad\Capabilities"
    );

    imp::unregister(&root).unwrap();
    assert!(!imp::is_registered(&root));
    let txt = CURRENT_USER
        .open(format!(r"{root}\Classes\.txt\OpenWithProgids"))
        .unwrap();
    assert!(
        txt.get_string(PROG_ID).is_err(),
        "the extension's own key stays"
    );
    imp::unregister(&root).unwrap();
    CURRENT_USER.remove_tree(&root).unwrap();
    // An extension no one chose Birchpad for.
    assert!(!is_default("birchpad-test-nothing"));
}

#[cfg(target_os = "macos")]
#[test]
fn finder_chooses_on_macos() {
    assert!(register(Path::new("/Applications/Birchpad.app")).is_err());
    unregister().unwrap();
    assert!(!is_default("txt"));
}
