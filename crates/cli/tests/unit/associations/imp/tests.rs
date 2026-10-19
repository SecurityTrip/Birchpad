use super::*;

#[test]
fn the_desktop_file_for_every_type() {
    let args = register_args();
    assert_eq!(&args[..2], ["default", DESKTOP_FILE]);
    assert_eq!(&args[2..], MIME_TYPES);
}

#[test]
fn what_the_programs_say() {
    run("true", &[]).unwrap();
    assert!(run("false", &[]).is_err(), "did not succeed");
    assert!(
        run("birchpad-no-such-program", &[]).is_err(),
        "cannot start"
    );
    assert!(answers_birchpad("echo", &[DESKTOP_FILE]));
    assert!(!answers_birchpad("echo", &["org.gnome.TextEditor.desktop"]));
    assert!(!answers_birchpad("echo", &[]), "nothing");
    assert!(!answers_birchpad("birchpad-no-such-program", &[]));
    // What only reads: nothing of Birchpad's to take away, and asking changes nothing.
    super::super::unregister().unwrap();
    let _ = super::super::is_default("txt");
}
