use super::*;

fn sample() -> Session {
    let mut session = Session::new();
    session.active_view = 1;
    session.documents = vec![
        SessionDocument {
            path: Some(PathBuf::from("/home/me/notes.txt")),
            backup: Some("notes.txt@1".into()),
            modified: true,
            encoding: Some("windows-1251".into()),
            line_ending: Some("crlf".into()),
            bookmarks: vec![2, 7],
            ..SessionDocument::default()
        },
        SessionDocument {
            untitled: Some(3),
            backup: Some("new 3@2".into()),
            modified: true,
            language: Some("rust".into()),
            ..SessionDocument::default()
        },
    ];
    session.main_view.tabs = vec![SessionTab {
        document: 0,
        selections: vec![[3, 9], [12, 12]],
        primary: 1,
        first_row: 4.5,
        scroll_x: 16.,
        folds: vec![1],
        pinned: true,
        color: Some(3),
        rtl: true,
    }];
    session.second_view = SessionView {
        active: 1,
        tabs: vec![
            SessionTab {
                document: 1,
                ..SessionTab::default()
            },
            SessionTab {
                document: 0,
                ..SessionTab::default()
            },
        ],
    };
    session
}

/// Does `io` to `path`, again while a virus scanner or indexer still holds the file for a
/// moment after it was written (Windows refuses with a sharing violation meanwhile).
fn retry(path: &Path, mut io: impl FnMut(&Path) -> std::io::Result<()>) {
    for _ in 0..100 {
        match io(path) {
            Err(error) if error.raw_os_error() == Some(32) => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            result => return result.unwrap(),
        }
    }
    panic!("{} stays in use", path.display());
}

fn remove(path: &Path) {
    retry(path, |path| fs::remove_file(path));
}

fn write(path: &Path, contents: &str) {
    retry(path, |path| fs::write(path, contents));
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("birchpad-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

#[test]
fn round_trips() {
    let session = sample();
    assert_eq!(Session::parse(&session.to_toml()).unwrap(), session);
    assert_eq!(
        session.backup_names().collect::<Vec<_>>(),
        ["notes.txt@1", "new 3@2"]
    );
}

#[test]
fn either_format_reads_with_or_without_a_byte_order_mark() {
    let toml = sample().to_toml();
    let xml = r#"<NotepadPlus><Session activeView="0"><mainView activeIndex="0">
            <File filename="C:\a.txt" /></mainView></Session></NotepadPlus>"#;
    for text in [toml.clone(), format!("\u{feff}{toml}")] {
        assert_eq!(Session::from_text(&text).unwrap(), sample());
    }
    for text in [xml.to_owned(), format!("\u{feff}  {xml}")] {
        let session = Session::from_text(&text).unwrap();
        assert_eq!(session.documents.len(), 1);
    }
    // One mark only: a second is text, which neither format starts with.
    assert!(Session::from_text(&format!("\u{feff}\u{feff}{xml}")).is_err());
    assert!(Session::from_text("").is_err());
    assert!(Session::from_text("\u{feff}").is_err());
}

#[test]
fn rejects_other_files_and_newer_formats() {
    assert!(Session::parse("recent-files = []").is_err());
    assert!(matches!(
        Session::parse("version = 99"),
        Err(SessionError::TooNew(99))
    ));
}

#[test]
fn a_damaged_session_falls_back_to_the_previous_one() {
    let dir = temp_dir("session");
    let path = dir.join("session.toml");
    let mut first = sample();
    first.active_view = 0;
    first.save(&path).unwrap();
    let second = sample();
    second.save(&path).unwrap();
    assert_eq!(Session::load(&path), Some(second));
    assert_eq!(Session::read(&previous(&path)).unwrap(), first);

    // Cut short (a crash in the middle of copying it, a full disk): the previous one loads.
    let text = fs::read_to_string(&path).unwrap();
    let cut = text.find("notes.txt@1").unwrap() + 4;
    write(&path, &text[..cut]);
    assert_eq!(Session::load(&path), Some(first.clone()));

    // Missing, as after a crash between the two renames of a save.
    remove(&path);
    assert_eq!(Session::load(&path), Some(first));
    // A temporary file left by an interrupted write is never read.
    fs::write(
        path.with_extension("toml.tmp"),
        "version = 1\nactive-view = 1",
    )
    .unwrap();
    remove(&previous(&path));
    assert_eq!(Session::load(&path), None);
    // Cleaning up may meet a scanner too; the temporary folder can stay.
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn private_files_replace_atomically() {
    let dir = temp_dir("private");
    let path = dir.join("backup").join("new 1@5");
    write_private(&path, b"one").unwrap();
    write_private(&path, b"two").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"two");
    assert!(!dir.join("backup").join("new 1@5.tmp").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(&dir.join("backup")), 0o700);
    }
    fs::remove_dir_all(&dir).unwrap();
}
