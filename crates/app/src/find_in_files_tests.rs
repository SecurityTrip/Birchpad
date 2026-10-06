use std::fs;

use birchpad_core::search::{Query, SearchMode};
use gpui_kit::{TestAppContext, VisualTestContext};

use super::*;
use crate::find::{FindBarEvent, FindOptions, FolderSearch};
use crate::workspace::tests::{active_text, open_workspace, secondary, tab_names};

const ANSI: Encoding = Encoding::Legacy("windows-1252");

fn searcher(pattern: &str) -> Searcher {
    Searcher::new(&Query {
        pattern: pattern.to_owned(),
        ..Query::default()
    })
    .unwrap()
}

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../io/tests/fixtures")
        .join(name);
    fs::read(path).unwrap()
}

/// `bytes` with each `from` replaced by `to`, as raw bytes.
fn replace_bytes(bytes: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut rest = bytes;
    while let Some(at) = rest.windows(from.len()).position(|window| window == from) {
        out.extend_from_slice(&rest[..at]);
        out.extend_from_slice(to);
        rest = &rest[at + from.len()..];
    }
    out.extend_from_slice(rest);
    out
}

fn encoded(text: &str, encoding: Encoding) -> Vec<u8> {
    birchpad_io::encode(&Rope::from_str(text), encoding, false).unwrap()
}

#[test]
fn skipped_files_are_described_in_one_phrase() {
    let mut counts = SkipCounts::default();
    assert_eq!(counts.describe(), "");
    counts.add(Skipped::Binary);
    assert_eq!(counts.describe(), "; 1 binary file skipped");
    counts.add(Skipped::Failed);
    counts.add(Skipped::Failed);
    counts.add(Skipped::ReadOnly);
    assert_eq!(
        counts.describe(),
        "; 2 unreadable files, 1 binary file and 1 read-only file skipped"
    );
    counts.add(Skipped::NotExact);
    assert!(
        counts
            .describe()
            .contains("1 not exactly decodable file and 1 read-only file skipped")
    );
}

#[test]
fn binary_means_a_zero_byte_in_the_first_64_kilobytes() {
    assert!(!is_binary(&Rope::new()));
    assert!(!is_binary(&Rope::from_str("plain text\n")));
    assert!(is_binary(&Rope::from_str("a\0b")));
    // The last byte looked at, and the first one past it, across the rope's chunks.
    let mut text = "x".repeat(BINARY_SAMPLE + 10);
    text.replace_range(BINARY_SAMPLE - 1..BINARY_SAMPLE, "\0");
    assert!(is_binary(&Rope::from_str(&text)));
    let mut text = "x".repeat(BINARY_SAMPLE + 10);
    text.replace_range(BINARY_SAMPLE..BINARY_SAMPLE + 1, "\0");
    assert!(!is_binary(&Rope::from_str(&text)));
}

#[test]
fn replacing_in_a_file_keeps_its_encoding_bom_and_line_endings() {
    let dir = tempfile::tempdir().unwrap();
    let cases = [
        (
            "windows-1251-crlf.txt",
            "Съешь",
            "Ешь",
            Encoding::Legacy("windows-1251"),
        ),
        (
            "koi8-r-lf.txt",
            "булок",
            "пирогов",
            Encoding::Legacy("KOI8-R"),
        ),
        ("utf-16be-bom-cr.txt", "quick", "slow", Encoding::Utf16Be),
        ("utf-8-bom-crlf.txt", "café", "кафе", Encoding::Utf8),
    ];
    for (name, from, to, encoding) in cases {
        let original = fixture(name);
        let path = dir.path().join(name);
        fs::write(&path, &original).unwrap();
        let count = replace_in_file(&path, &searcher(from), to, ANSI, None).unwrap();
        assert_eq!(count, 1, "{name}");
        let expected = replace_bytes(&original, &encoded(from, encoding), &encoded(to, encoding));
        assert_eq!(fs::read(&path).unwrap(), expected, "{name}");
    }
}

#[test]
fn files_without_a_match_are_left_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, "nothing here").unwrap();
    // Read-only too: a file with nothing to replace is not "skipped".
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions.clone()).unwrap();
    assert_eq!(
        replace_in_file(&path, &searcher("needle"), "x", ANSI, None),
        Ok(0)
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "nothing here");
    #[expect(clippy::permissions_set_readonly_false, reason = "a temporary file")]
    permissions.set_readonly(false);
    fs::set_permissions(&path, permissions).unwrap();
}

#[test]
fn files_that_cannot_be_changed_exactly_are_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let write = |name: &str, bytes: &[u8]| {
        let path = dir.path().join(name);
        fs::write(&path, bytes).unwrap();
        path
    };
    let a = searcher("a");

    // Binary, a decoding problem, characters the encoding lacks.
    let binary = write("nul.txt", &fixture("nul-bytes.txt"));
    assert_eq!(
        replace_in_file(&binary, &a, "x", ANSI, None),
        Err(Skipped::Binary)
    );
    let surrogate = write("lone.txt", &fixture("utf-16le-lone-surrogate.txt"));
    assert_eq!(
        replace_in_file(&surrogate, &a, "x", ANSI, None),
        Err(Skipped::NotExact)
    );
    let western = write("cp1252.txt", b"caf\xe9 a");
    assert_eq!(
        replace_in_file(&western, &a, "Ж", ANSI, None),
        Err(Skipped::NotExact)
    );
    // Each is still what it was.
    assert_eq!(fs::read(&binary).unwrap(), fixture("nul-bytes.txt"));
    assert_eq!(fs::read(&western).unwrap(), b"caf\xe9 a");
    // In the same encoding the replacement fits.
    assert_eq!(replace_in_file(&western, &a, "b", ANSI, None), Ok(2));
    assert_eq!(fs::read(&western).unwrap(), b"cbf\xe9 b");

    let read_only = write("ro.txt", b"a");
    let mut permissions = fs::metadata(&read_only).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&read_only, permissions.clone()).unwrap();
    assert_eq!(
        replace_in_file(&read_only, &a, "x", ANSI, None),
        Err(Skipped::ReadOnly)
    );
    #[expect(clippy::permissions_set_readonly_false, reason = "a temporary file")]
    permissions.set_readonly(false);
    fs::set_permissions(&read_only, permissions).unwrap();

    let missing = dir.path().join("missing.txt");
    assert_eq!(
        replace_in_file(&missing, &a, "x", ANSI, None),
        Err(Skipped::Failed)
    );
}

// --- In the window -------------------------------------------------------------------------

/// Sets up the Find in Files tab: the text to find, the folder and the filters.
fn search_files(
    workspace: &Entity<Workspace>,
    pattern: &str,
    directory: &Path,
    filters: &str,
    cx: &mut VisualTestContext,
) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.find_bar.update(cx, |bar, cx| {
            bar.show(
                FindTab::FindInFiles,
                Some(pattern.to_owned()),
                None,
                window,
                cx,
            );
            bar.set_directory(directory, window, cx);
            bar.set_filters(filters, window, cx);
        });
    });
}

fn act(workspace: &Entity<Workspace>, event: FindBarEvent, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        let bar = workspace.find_bar.clone();
        workspace.on_find_bar_event(&bar, &event, window, cx);
    });
}

fn status(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    workspace.read_with(cx, |workspace, cx| {
        workspace.find_bar.read(cx).status_text()
    })
}

fn rows(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<String> {
    workspace.read_with(cx, |workspace, cx| {
        workspace.search_results.read(cx).rows_text()
    })
}

/// Find All in files, run to the end.
fn find_all(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<String> {
    act(workspace, FindBarEvent::FindInFiles, cx);
    cx.run_until_parked();
    status(workspace, cx)
}

fn shown(path: &Path) -> String {
    path.display().to_string()
}

fn open(workspace: &Entity<Workspace>, path: &Path, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(path, window, cx)
    });
    cx.run_until_parked();
}

fn selection(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> (usize, usize) {
    workspace.read_with(cx, |workspace, cx| {
        let range = workspace
            .active_view(cx)
            .unwrap()
            .read(cx)
            .selection
            .primary();
        (range.anchor, range.head)
    })
}

#[gpui_kit::test]
fn finds_in_the_files_that_the_filters_take(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "foo\nbar").unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    fs::write(dir.path().join("sub/b.txt"), "x foo foo\r\nFOO").unwrap();
    fs::write(dir.path().join("c.rs"), "foo").unwrap();
    fs::write(dir.path().join("d.txt"), "nothing").unwrap();
    let (workspace, cx) = open_workspace(cx);
    search_files(&workspace, "foo", dir.path(), "*.txt", cx);
    assert_eq!(
        find_all(&workspace, cx).as_deref(),
        Some("Find in Files: 4 hits in 2 files of 3 searched")
    );
    assert_eq!(
        rows(&workspace, cx),
        [
            "Search \"foo\" (4 hits in 2 files of 3 searched)".to_owned(),
            format!("  {} (1)", shown(&dir.path().join("a.txt"))),
            "    Line 1: foo".to_owned(),
            format!("  {} (3)", shown(&dir.path().join("sub").join("b.txt"))),
            "    Line 1: x foo foo".to_owned(),
            "    Line 2: FOO".to_owned(),
        ]
    );

    // Without subfolders, and with options of the panel: match case.
    workspace.update(cx, |workspace, cx| {
        workspace.find_bar.update(cx, |bar, _| {
            bar.folder.subfolders = false;
            bar.options.match_case = true;
        });
    });
    search_files(&workspace, "FOO", dir.path(), "", cx);
    assert_eq!(
        find_all(&workspace, cx).as_deref(),
        Some("Find in Files: 0 hits in 0 files of 3 searched"),
        "a.txt, c.rs and d.txt, without sub/b.txt"
    );
    assert_eq!(
        rows(&workspace, cx)[0],
        "Search \"FOO\" (0 hits in 0 files of 3 searched)",
        "an empty search is listed too, above the earlier one"
    );
}

#[gpui_kit::test]
fn f4_opens_a_file_result_and_selects_its_match(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.txt");
    fs::write(&path, "first\nthe needle here\nneedle").unwrap();
    let (workspace, cx) = open_workspace(cx);
    search_files(&workspace, "needle", dir.path(), "", cx);
    find_all(&workspace, cx);
    cx.simulate_keystrokes("f4");
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx), ["notes.txt"], "replaces new 1");
    assert_eq!(selection(&workspace, cx), (10, 16));
    cx.simulate_keystrokes("f4");
    assert_eq!(selection(&workspace, cx), (22, 28));
    // The file changed meanwhile: a result past its end goes to the end.
    workspace.update_in(cx, |workspace, _, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.update(cx, |view, cx| view.select_in_line(99, 30..40, cx));
    });
    let len = active_text(&workspace, cx).len();
    assert_eq!(selection(&workspace, cx), (len, len));
}

#[gpui_kit::test]
fn open_documents_are_searched_and_changed_as_they_are_in_their_tabs(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("open.txt");
    fs::write(&path, "saved text").unwrap();
    let (workspace, cx) = open_workspace(cx);
    open(&workspace, &path, cx);
    cx.simulate_input("unsaved word ");
    search_files(&workspace, "word", dir.path(), "", cx);
    assert_eq!(
        find_all(&workspace, cx).as_deref(),
        Some("Find in Files: 1 hit in 1 file of 1 searched")
    );
    // Its result is the tab, not the file.
    cx.simulate_keystrokes("f4");
    assert_eq!(selection(&workspace, cx), (8, 12));

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.find_bar.update(cx, |bar, cx| {
            bar.replace_input_for_tests("phrase", window, cx);
        });
    });
    act(&workspace, FindBarEvent::ReplaceInFiles, cx);
    cx.simulate_prompt_answer("Replace");
    cx.run_until_parked();
    assert_eq!(
        status(&workspace, cx).as_deref(),
        Some("Replace in Files: 1 occurrence replaced in 1 file")
    );
    assert_eq!(active_text(&workspace, cx), "unsaved phrase saved text");
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "saved text",
        "not saved"
    );
    cx.simulate_keystrokes(&secondary("z"));
    assert_eq!(active_text(&workspace, cx), "unsaved word saved text");
}

#[gpui_kit::test]
fn replace_in_files_asks_first_and_changes_files_on_disk(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.log");
    fs::write(&a, "one two one").unwrap();
    fs::write(&b, "one").unwrap();
    let (workspace, cx) = open_workspace(cx);
    search_files(&workspace, "one", dir.path(), "*.txt", cx);
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.find_bar.update(cx, |bar, cx| {
            bar.replace_input_for_tests("1", window, cx);
        });
    });

    // Cancel changes nothing.
    act(&workspace, FindBarEvent::ReplaceInFiles, cx);
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&a).unwrap(), "one two one");

    act(&workspace, FindBarEvent::ReplaceInFiles, cx);
    cx.simulate_prompt_answer("Replace");
    cx.run_until_parked();
    assert_eq!(
        status(&workspace, cx).as_deref(),
        Some("Replace in Files: 2 occurrences replaced in 1 file")
    );
    assert_eq!(fs::read_to_string(&a).unwrap(), "1 two 1");
    assert_eq!(fs::read_to_string(&b).unwrap(), "one", "not in the filters");

    // Nothing left to replace: a failure, and no file is touched.
    act(&workspace, FindBarEvent::ReplaceInFiles, cx);
    cx.simulate_prompt_answer("Replace");
    cx.run_until_parked();
    assert_eq!(
        status(&workspace, cx).as_deref(),
        Some("Replace in Files: 0 occurrences replaced in 0 files")
    );
}

#[gpui_kit::test]
fn folders_and_filters_that_cannot_be_searched_are_reported(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("file.txt"), "x").unwrap();
    let (workspace, cx) = open_workspace(cx);

    search_files(&workspace, "x", Path::new("  "), "", cx);
    assert_eq!(
        find_all(&workspace, cx).as_deref(),
        Some("Find in Files: choose a folder")
    );
    let missing = dir.path().join("missing");
    search_files(&workspace, "x", &missing, "", cx);
    assert_eq!(
        find_all(&workspace, cx),
        Some(format!("Find in Files: {} does not exist", shown(&missing)))
    );
    let file = dir.path().join("file.txt");
    search_files(&workspace, "x", &file, "", cx);
    assert_eq!(
        find_all(&workspace, cx),
        Some(format!("Find in Files: {} is not a folder", shown(&file)))
    );
    search_files(&workspace, "x", dir.path(), "*.txt !", cx);
    assert!(
        find_all(&workspace, cx)
            .is_some_and(|status| status.starts_with("Find in Files: \"!\" leaves out nothing"))
    );
    search_files(&workspace, "", dir.path(), "", cx);
    assert_eq!(
        find_all(&workspace, cx).as_deref(),
        Some("Find: the search text is empty")
    );
    let regex = FindOptions {
        mode: SearchMode::Regex,
        ..FindOptions::default()
    };
    workspace.update(cx, |workspace, cx| {
        workspace.find_bar.update(cx, |bar, _| bar.options = regex);
    });
    search_files(&workspace, "(", dir.path(), "", cx);
    assert!(
        find_all(&workspace, cx)
            .is_some_and(|status| status.starts_with("Find: invalid regular expression"))
    );
    assert!(rows(&workspace, cx).is_empty(), "none of them searched");
    // Replace in Files refuses the same way, before asking anything.
    search_files(&workspace, "x", &missing, "", cx);
    act(&workspace, FindBarEvent::ReplaceInFiles, cx);
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
}

#[gpui_kit::test]
fn empty_folders_binary_files_and_stopping(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let empty = dir.path().join("empty");
    fs::create_dir(&empty).unwrap();
    let (workspace, cx) = open_workspace(cx);
    search_files(&workspace, "a", &empty, "", cx);
    assert_eq!(
        find_all(&workspace, cx).as_deref(),
        Some("Find in Files: 0 hits in 0 files of 0 searched")
    );

    fs::write(dir.path().join("nul.bin"), fixture("nul-bytes.txt")).unwrap();
    fs::write(dir.path().join("text.txt"), "a").unwrap();
    search_files(&workspace, "a", dir.path(), "", cx);
    assert_eq!(
        find_all(&workspace, cx).as_deref(),
        Some("Find in Files: 1 hit in 1 file of 2 searched; 1 binary file skipped")
    );

    // Stop before the folder has been listed: nothing is searched.
    act(&workspace, FindBarEvent::FindInFiles, cx);
    let running = workspace.read_with(cx, |workspace, cx| workspace.files_running(cx));
    assert!(running);
    // A second start while one runs is ignored.
    act(&workspace, FindBarEvent::FindInFiles, cx);
    act(&workspace, FindBarEvent::StopFiles, cx);
    cx.run_until_parked();
    assert_eq!(
        status(&workspace, cx).as_deref(),
        Some("Find in Files: 0 hits in 0 files of 0 searched (stopped)")
    );
    let running = workspace.read_with(cx, |workspace, cx| workspace.files_running(cx));
    assert!(!running);
    // Stop with nothing running does nothing.
    act(&workspace, FindBarEvent::StopFiles, cx);
}

#[gpui_kit::test]
fn the_folder_follows_the_current_document_and_is_remembered(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let path = dir.path().join("doc.txt");
    fs::write(&path, "x").unwrap();
    let (workspace, cx) = open_workspace(cx);
    let directory = |cx: &mut VisualTestContext| {
        workspace.read_with(cx, |workspace, cx| {
            workspace.find_bar.read(cx).directory(cx)
        })
    };

    // An untitled document has no folder: the field stays empty.
    cx.simulate_keystrokes(&secondary("shift-f"));
    assert_eq!(directory(cx), "");
    open(&workspace, &path, cx);
    // An empty field is filled in from the document even without Follow current doc.
    cx.simulate_keystrokes(&secondary("shift-f"));
    assert_eq!(directory(cx), shown(dir.path()));
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.find_bar.update(cx, |bar, cx| {
            bar.set_directory(elsewhere.path(), window, cx);
        });
    });
    cx.simulate_keystrokes(&secondary("shift-f"));
    assert_eq!(
        directory(cx),
        shown(elsewhere.path()),
        "a typed folder stays"
    );
    workspace.update(cx, |workspace, cx| {
        workspace.find_bar.update(cx, |bar, _| {
            bar.folder = FolderSearch {
                subfolders: false,
                hidden: true,
                follow_current_document: true,
            };
        });
    });
    cx.simulate_keystrokes(&secondary("shift-f"));
    assert_eq!(directory(cx), shown(dir.path()), "it follows");

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.find_bar.update(cx, |bar, cx| {
            bar.show(FindTab::FindInFiles, Some("x".into()), None, window, cx);
            bar.set_filters("*.txt", window, cx);
        });
    });
    assert_eq!(
        find_all(&workspace, cx).as_deref(),
        Some("Find in Files: 1 hit in 1 file of 1 searched")
    );
    let remembered = cx.update(|_, cx| AppState::global(cx).state.find_in_files.clone());
    assert_eq!(
        remembered,
        FindInFilesState {
            filters: "*.txt".into(),
            directory: Some(dir.path().to_owned()),
            subfolders: false,
            hidden: true,
            follow_current_document: true,
        }
    );
}
