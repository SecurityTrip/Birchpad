//! Fixture tests: real files in every supported encoding, BOM and line-ending combination are
//! detected correctly and save back byte for byte.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;

use birchpad_core::{
    Document, Edit, Encoding, LineEnding, Rope, Selection, Transaction, UndoGrouping,
};
use birchpad_io::{
    DecodeProblem, DetectedBy, LoadOptions, LoadedFile, SaveError, encode, load,
    pending_recoveries, save,
};

const ANSI: Encoding = Encoding::Legacy("windows-1252");

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn open(name: &str) -> LoadedFile {
    open_as(name, None)
}

fn open_as(name: &str, encoding: Option<Encoding>) -> LoadedFile {
    let options = LoadOptions {
        encoding,
        ansi: ANSI,
    };
    load(&fixture(name), options, &AtomicU64::new(0)).unwrap()
}

fn document(file: &LoadedFile) -> Document {
    Document::with_format(file.text.clone(), file.format)
}

fn save_bytes(doc: &Document) -> Vec<u8> {
    let format = doc.format();
    encode(doc.text(), format.encoding, format.bom).unwrap()
}

/// (file name prefix, encoding, BOM)
const MATRIX: &[(&str, Encoding, bool)] = &[
    ("utf-8", Encoding::Utf8, false),
    ("utf-8-bom", Encoding::Utf8, true),
    ("utf-16le-bom", Encoding::Utf16Le, true),
    ("utf-16be-bom", Encoding::Utf16Be, true),
    ("utf-16le", Encoding::Utf16Le, false),
    ("utf-16be", Encoding::Utf16Be, false),
    ("windows-1251", Encoding::Legacy("windows-1251"), false),
    ("koi8-r", Encoding::Legacy("KOI8-R"), false),
    ("ibm866", Encoding::Legacy("IBM866"), false),
    ("windows-1252", Encoding::Legacy("windows-1252"), false),
    ("shift_jis", Encoding::Legacy("Shift_JIS"), false),
];

const EOLS: [(&str, LineEnding); 3] = [
    ("crlf", LineEnding::CrLf),
    ("lf", LineEnding::Lf),
    ("cr", LineEnding::Cr),
];

#[test]
fn matrix_is_detected_and_saves_back_identically() {
    for (prefix, encoding, bom) in MATRIX {
        for (eol_name, eol) in EOLS {
            let name = format!("{prefix}-{eol_name}.txt");
            let file = open(&name);
            assert_eq!(file.format.encoding, *encoding, "{name}: encoding");
            assert_eq!(file.format.bom, *bom, "{name}: BOM");
            assert_eq!(file.problem, None, "{name}: problem");
            let doc = document(&file);
            assert_eq!(doc.line_ending(), eol, "{name}: line ending");
            assert_eq!(
                save_bytes(&doc),
                fs::read(fixture(&name)).unwrap(),
                "{name}: open and save must not change a byte"
            );
        }
    }
}

#[test]
fn edits_change_only_the_edited_bytes() {
    for (prefix, encoding, _) in MATRIX {
        for (eol_name, _) in EOLS {
            let name = format!("{prefix}-{eol_name}.txt");
            let original = fs::read(fixture(&name)).unwrap();
            let file = open(&name);
            let mut doc = document(&file);

            // Insert at the start of the second line some text every encoding can represent.
            let text = doc.text().clone();
            let pos = birchpad_core::motion::line_range_with_break(&text, 0).end;
            let inserted = "XYZ 123";
            let transaction =
                Transaction::from_edits(&text, [Edit::insert(pos, inserted)]).unwrap();
            doc.apply(&transaction, &Selection::point(pos), UndoGrouping::NewStep);

            let format = doc.format();
            let head = encode(&text.slice(..pos).into(), format.encoding, format.bom).unwrap();
            let middle = encode(&Rope::from_str(inserted), format.encoding, false).unwrap();
            let saved = save_bytes(&doc);
            let mut expected = original[..head.len()].to_vec();
            expected.extend_from_slice(&middle);
            expected.extend_from_slice(&original[head.len()..]);
            assert_eq!(saved, expected, "{name} ({encoding:?})");
        }
    }
}

#[test]
fn eol_conversion_touches_only_line_breaks() {
    let file = open("utf-8-bom-crlf.txt");
    let mut doc = document(&file);
    let conversion = doc.convert_line_endings(LineEnding::Lf);
    doc.apply(&conversion, &Selection::point(0), UndoGrouping::NewStep);
    let saved = save_bytes(&doc);
    assert_eq!(saved, fs::read(fixture("utf-8-bom-lf.txt")).unwrap());
    doc.undo();
    assert_eq!(
        save_bytes(&doc),
        fs::read(fixture("utf-8-bom-crlf.txt")).unwrap()
    );
}

#[test]
fn special_files() {
    let empty = open("empty.txt");
    assert_eq!(
        (empty.text.len(), empty.format.encoding, empty.format.bom),
        (0, Encoding::Utf8, false)
    );

    let bom_only = open("bom-only.txt");
    assert_eq!((bom_only.text.len(), bom_only.format.bom), (0, true));
    assert_eq!(save_bytes(&document(&bom_only)), b"\xEF\xBB\xBF");

    let utf16_bom_only = open("utf-16le-bom-only.txt");
    assert_eq!(utf16_bom_only.format.encoding, Encoding::Utf16Le);
    assert_eq!(save_bytes(&document(&utf16_bom_only)), b"\xFF\xFE");

    let mixed = open("mixed-eol.txt");
    let doc = document(&mixed);
    assert_eq!(doc.line_ending(), LineEnding::CrLf, "first break decides");
    assert_eq!(
        save_bytes(&doc),
        fs::read(fixture("mixed-eol.txt")).unwrap()
    );

    let nul = open("nul-bytes.txt");
    assert_eq!(nul.format.encoding, Encoding::Utf8);
    assert_eq!(
        save_bytes(&document(&nul)),
        fs::read(fixture("nul-bytes.txt")).unwrap()
    );
}

#[test]
fn invalid_bytes_are_reported_not_hidden() {
    // Not valid UTF-8, so detection picks the ANSI code page, which can represent the bytes.
    let detected = open("invalid-utf8.txt");
    assert_eq!(detected.detected_by, Some(DetectedBy::Ansi));
    assert_eq!(detected.problem, None);
    assert_eq!(
        save_bytes(&document(&detected)),
        fs::read(fixture("invalid-utf8.txt")).unwrap()
    );

    // Forced to UTF-8 ("Encode in UTF-8"), the bad byte is reported with its position.
    let forced = open_as("invalid-utf8.txt", Some(Encoding::Utf8));
    assert_eq!(forced.problem, Some(DecodeProblem::Malformed { offset: 3 }));

    let truncated = open("utf-16le-truncated.txt");
    assert_eq!(truncated.format.encoding, Encoding::Utf16Le);
    assert!(matches!(
        truncated.problem,
        Some(DecodeProblem::Malformed { .. })
    ));

    let surrogate = open("utf-16le-lone-surrogate.txt");
    assert_eq!(
        surrogate.problem,
        Some(DecodeProblem::Malformed { offset: 4 })
    );
}

#[test]
fn reinterpreting_reads_the_same_bytes_differently() {
    let as_1251 = open_as("koi8-r-lf.txt", Some(Encoding::Legacy("windows-1251")));
    let as_koi8 = open_as("koi8-r-lf.txt", Some(Encoding::Legacy("KOI8-R")));
    assert_ne!(as_1251.text, as_koi8.text);
    assert!(as_koi8.text.to_string().starts_with("Съешь"));
    // Either way the bytes survive unchanged.
    for file in [&as_1251, &as_koi8] {
        assert_eq!(
            save_bytes(&document(file)),
            fs::read(fixture("koi8-r-lf.txt")).unwrap()
        );
    }
}

#[test]
fn unencodable_text_blocks_saving() {
    let mut doc = document(&open("windows-1251-crlf.txt"));
    let transaction = Transaction::from_edits(doc.text(), [Edit::insert(0, "日本 ")]).unwrap();
    doc.apply(&transaction, &Selection::point(0), UndoGrouping::NewStep);
    let format = doc.format();
    let error = encode(doc.text(), format.encoding, format.bom).unwrap_err();
    assert_eq!(error.count, 2);
    assert_eq!(error.samples, [(0, '日'), (3, '本')]);
}

// --- Saving to disk ---------------------------------------------------------------------------

fn temp_dir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

#[test]
fn save_writes_in_place_and_cleans_up() {
    let dir = temp_dir();
    let path = dir.path().join("a.txt");
    let recovery = dir.path().join("recovery");
    fs::write(&path, b"old content that is longer").unwrap();

    let written = save(&path, b"new", Some(&recovery)).unwrap();
    assert_eq!(written, path);
    assert_eq!(fs::read(&path).unwrap(), b"new");
    assert!(pending_recoveries(&recovery).is_empty());

    save(&path, b"grown content, longer than before", Some(&recovery)).unwrap();
    assert_eq!(
        fs::read(&path).unwrap(),
        b"grown content, longer than before"
    );

    let created = dir.path().join("new.txt");
    save(&created, b"fresh", Some(&recovery)).unwrap();
    assert_eq!(fs::read(&created).unwrap(), b"fresh");
}

#[test]
fn read_only_files_are_refused() {
    let dir = temp_dir();
    let path = dir.path().join("ro.txt");
    fs::write(&path, b"keep me").unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions.clone()).unwrap();

    let error = save(&path, b"changed", None).unwrap_err();
    assert!(matches!(error, SaveError::ReadOnly(_)), "{error}");
    assert_eq!(fs::read(&path).unwrap(), b"keep me");

    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    fs::set_permissions(&path, permissions).unwrap();
}

#[cfg(unix)]
#[test]
fn links_keep_their_identity() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

    let dir = temp_dir();
    let real = dir.path().join("real.txt");
    fs::write(&real, b"original").unwrap();
    fs::set_permissions(&real, fs::Permissions::from_mode(0o640)).unwrap();
    let inode = fs::metadata(&real).unwrap().ino();

    // Saving through a symbolic link writes the target and keeps the link.
    let link = dir.path().join("link.txt");
    symlink("real.txt", &link).unwrap();
    let written = save(&link, b"via symlink", None).unwrap();
    assert_eq!(written, dir.path().join("real.txt"));
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(&real).unwrap(), b"via symlink");

    // Saving through a hard link is visible through the other name: same inode, same mode.
    let hard = dir.path().join("hard.txt");
    fs::hard_link(&real, &hard).unwrap();
    save(&hard, b"via hard link", None).unwrap();
    assert_eq!(fs::read(&real).unwrap(), b"via hard link");
    let metadata = fs::metadata(&real).unwrap();
    assert_eq!(metadata.ino(), inode);
    assert_eq!(metadata.permissions().mode() & 0o777, 0o640);

    // A dangling link is resolved to the file it points at, which is created.
    let dangling = dir.path().join("dangling.txt");
    symlink("missing.txt", &dangling).unwrap();
    save(&dangling, b"created", None).unwrap();
    assert_eq!(
        fs::read(dir.path().join("missing.txt")).unwrap(),
        b"created"
    );
}

/// Saving writes the file in place (ADR 0007): its ACL, alternate data streams (the "downloaded
/// from the internet" mark among them), creation time and hard links stay.
#[cfg(windows)]
#[test]
fn acls_streams_and_links_stay_on_windows() {
    use std::process::Command;

    let dir = temp_dir();
    let path = dir.path().join("a.txt");
    fs::write(&path, b"original content").unwrap();
    let stream = |name: &str| PathBuf::from(format!("{}:{name}", path.display()));
    let zone = b"[ZoneTransfer]\r\nZoneId=3\r\n";
    fs::write(stream("Zone.Identifier"), zone).unwrap();
    fs::write(stream("extra"), b"kept").unwrap();
    // An explicit entry for Everyone (by SID, so the system language does not matter), which a
    // file created anew would not have: it would only inherit the folder's entries.
    let granted = Command::new("icacls")
        .arg(&path)
        .args(["/grant", "*S-1-1-0:(R)"])
        .output()
        .unwrap();
    assert!(granted.status.success(), "{granted:?}");
    let acl = || {
        let output = Command::new("icacls").arg(&path).output().unwrap();
        assert!(output.status.success(), "{output:?}");
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    let acl_before = acl();
    let created = fs::metadata(&path).unwrap().created().unwrap();
    let hard = dir.path().join("hard.txt");
    fs::hard_link(&path, &hard).unwrap();

    for content in [&b"short"[..], b"longer than the original content was"] {
        save(&path, content, Some(&dir.path().join("recovery"))).unwrap();
        assert_eq!(fs::read(&path).unwrap(), content);
        assert_eq!(acl(), acl_before);
        assert_eq!(fs::read(stream("Zone.Identifier")).unwrap(), zone);
        assert_eq!(fs::read(stream("extra")).unwrap(), b"kept");
        assert_eq!(fs::metadata(&path).unwrap().created().unwrap(), created);
        assert_eq!(fs::read(&hard).unwrap(), content, "the same file");
    }

    // Saving through the other name of the file changes it under both.
    save(&hard, b"via the hard link", None).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"via the hard link");
    assert_eq!(acl(), acl_before);

    // Symbolic links need Developer Mode or administrator rights to create.
    let link = dir.path().join("link.txt");
    if std::os::windows::fs::symlink_file("a.txt", &link).is_ok() {
        let written = save(&link, b"via symlink", None).unwrap();
        assert_eq!(written, path);
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(&path).unwrap(), b"via symlink");
        assert_eq!(fs::read(stream("extra")).unwrap(), b"kept");
    }
}

#[test]
fn leftover_recovery_copies_are_found() {
    let dir = temp_dir();
    fs::write(dir.path().join("1-2-a.txt.data"), b"saved text").unwrap();
    fs::write(dir.path().join("1-2-a.txt.path"), "/home/user/a.txt").unwrap();
    fs::write(dir.path().join("orphan.data"), b"no path file").unwrap();
    let found = pending_recoveries(dir.path());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].original, Path::new("/home/user/a.txt"));
    birchpad_io::discard_recovery(&found[0]);
    assert!(pending_recoveries(dir.path()).is_empty());
}

#[test]
fn saving_where_it_cannot_fails_and_leaves_files_alone() {
    let dir = temp_dir();
    // A folder that does not exist: nothing is created.
    let nowhere = dir.path().join("missing").join("a.txt");
    let error = save(&nowhere, b"text", None).unwrap_err();
    assert!(matches!(error, SaveError::Io { .. }), "{error}");
    assert!(!nowhere.exists() && !dir.path().join("missing").exists());

    // A folder in place of the file.
    let folder = dir.path().join("folder");
    fs::create_dir(&folder).unwrap();
    let error = save(&folder, b"text", None).unwrap_err();
    assert!(matches!(error, SaveError::NotAFile(_)), "{error}");

    // The recovery copy cannot be written (its folder is a file): the file is not touched.
    let path = dir.path().join("keep.txt");
    fs::write(&path, b"precious").unwrap();
    let blocked = dir.path().join("recovery-is-a-file");
    fs::write(&blocked, b"").unwrap();
    assert!(save(&path, b"new text", Some(&blocked)).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"precious");
}

#[test]
fn saving_nothing_empties_the_file() {
    let dir = temp_dir();
    let path = dir.path().join("a.txt");
    fs::write(&path, b"something").unwrap();
    save(&path, b"", None).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"");
    // And an empty new file, then the same length again.
    let created = dir.path().join("empty.txt");
    save(&created, b"", None).unwrap();
    assert_eq!(fs::metadata(&created).unwrap().len(), 0);
    save(&path, b"123456789", None).unwrap();
    save(&path, b"abcdefghi", None).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"abcdefghi");
}

#[test]
fn recoveries_of_a_missing_folder_are_none() {
    let dir = temp_dir();
    assert!(pending_recoveries(&dir.path().join("never-created")).is_empty());
    assert!(pending_recoveries(dir.path()).is_empty(), "an empty folder");
}

#[test]
fn missing_and_directory_paths_fail_cleanly() {
    let dir = temp_dir();
    let options = LoadOptions {
        encoding: None,
        ansi: ANSI,
    };
    let progress = AtomicU64::new(0);
    assert!(matches!(
        load(&dir.path().join("nope.txt"), options, &progress),
        Err(birchpad_io::ReadError::NotFound(_))
    ));
    assert!(load(dir.path(), options, &progress).is_err());
}

/// `cargo test -p birchpad-io --release -- --ignored --nocapture`
#[test]
#[ignore = "performance check; run in release"]
fn large_file_loads_quickly() {
    use std::time::Instant;

    let dir = temp_dir();
    let path = dir.path().join("large.txt");
    let line =
        "The quick brown fox jumps over the lazy dog; съешь же ещё этих булок 0123456789\r\n";
    let target = 100 * 1024 * 1024;
    let mut text = String::with_capacity(target + line.len());
    while text.len() < target {
        text.push_str(line);
    }
    fs::write(&path, &text).unwrap();
    drop(text);

    let started = Instant::now();
    let file = load(
        &path,
        LoadOptions {
            encoding: None,
            ansi: ANSI,
        },
        &AtomicU64::new(0),
    )
    .unwrap();
    let doc = document(&file);
    let elapsed = started.elapsed();
    eprintln!(
        "loaded {} MB, {} lines in {elapsed:?}",
        doc.text().len() / 1024 / 1024,
        birchpad_core::motion::line_count(doc.text())
    );
    assert_eq!(file.format.encoding, Encoding::Utf8);
    assert!(elapsed.as_secs_f64() < 1.5, "took {elapsed:?}");
}
