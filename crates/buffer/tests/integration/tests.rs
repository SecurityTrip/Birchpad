use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use birchpad_buffer::*;
use birchpad_core::{Document, Encoding, Rope, Selection, Transaction, UndoGrouping};
use birchpad_syntax::UserLanguage;
use birchpad_ui::AppState;
use gpui_kit::{App, AppContext as _, Entity, TestAppContext};

/// The settings every buffer reads: the defaults, with no settings folder.
fn init(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(AppState::new(
            birchpad_config::resolve(birchpad_config::Sources::default()),
            birchpad_config::ConfigPaths::default(),
        ));
    });
}

/// What a buffer told since it was made, by the names of its events.
fn events(buffer: &Entity<Buffer>, cx: &mut TestAppContext) -> Rc<RefCell<Vec<&'static str>>> {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let told = seen.clone();
    cx.update(|cx| {
        cx.subscribe(buffer, move |_, event: &BufferEvent, _| {
            told.borrow_mut().push(match event {
                BufferEvent::Edited { .. } => "edited",
                BufferEvent::Reloaded { .. } => "reloaded",
                BufferEvent::FollowEnd => "follow-end",
                BufferEvent::StateChanged => "state",
                BufferEvent::MarksChanged => "marks",
                BufferEvent::SyntaxChanged => "syntax",
                BufferEvent::LoadFailed(_) => "load-failed",
            });
        })
        .detach();
    });
    seen
}

fn text(buffer: &Entity<Buffer>, cx: &mut TestAppContext) -> String {
    buffer.read_with(cx, |buffer, _| buffer.doc().text().to_string())
}

/// Types `typed` at `pos`, as a view would.
fn type_at(buffer: &Entity<Buffer>, pos: usize, typed: &str, cx: &mut TestAppContext) {
    buffer.update(cx, |buffer, cx| {
        let selection = Selection::point(pos);
        let transaction =
            Transaction::replace_selections(buffer.doc().text(), &selection, typed).unwrap();
        buffer.apply(transaction, &selection, UndoGrouping::NewStep, None, cx);
    });
}

#[test]
fn mark_all_can_bookmark_the_lines_of_its_matches() {
    let text = Rope::from_str("a\nb b\nc\nb");
    let mut marks = DocumentMarks::default();
    marks.bookmarks.add(&text, 0);
    marks.bookmark_lines_of(&text, &[2..3, 4..5, 8..9]);
    assert_eq!(marks.bookmarks.lines(&text), [0, 1, 3]);
}

#[gpui_kit::test]
fn documents_are_named_by_their_file_their_title_or_their_number(cx: &mut TestAppContext) {
    init(cx);
    let buffer = cx.new(|_| Buffer::untitled(3));
    let seen = events(&buffer, cx);
    buffer.read_with(cx, |buffer, _| {
        assert_eq!(buffer.display_name(), "new 3");
        assert_eq!(buffer.untitled_number(), Some(3));
        assert_eq!((buffer.path(), buffer.title()), (None, None));
        assert!(!buffer.is_modified());
        assert_eq!(buffer.language_key(), "text");
        assert_eq!(buffer.read_only(), None);
    });

    // A name given to an untitled document, and taken back.
    buffer.update(cx, |buffer, cx| buffer.set_title(Some("Ideas".into()), cx));
    buffer.read_with(cx, |buffer, _| {
        assert_eq!(buffer.display_name(), "Ideas");
        assert_eq!(buffer.title(), Some("Ideas"));
    });
    buffer.update(cx, |buffer, cx| buffer.set_title(None, cx));
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.display_name()),
        "new 3"
    );

    // With a file, the file's name wins over a title, which is no longer its name.
    let path = PathBuf::from("folder").join("notes.rs");
    buffer.update(cx, |buffer, cx| {
        buffer.set_title(Some("Ideas".into()), cx);
        buffer.renamed(path.clone(), cx);
    });
    buffer.read_with(cx, |buffer, _| {
        assert_eq!(buffer.display_name(), "notes.rs");
        assert_eq!(buffer.path(), Some(path.as_path()));
        assert_eq!(buffer.title(), None);
        assert_eq!(buffer.language_key(), "rust", "by the file's extension");
        assert_eq!(buffer.disk(), None, "a file that is not there has no stamp");
    });
    // Each of the four changes of the name is told, and the language that came with the file.
    let seen = seen.borrow();
    let count = |name: &str| seen.iter().filter(|event| **event == name).count();
    assert!(count("state") >= 4, "{seen:?}");
    assert_eq!(count("syntax"), 1, "{seen:?}");
    assert_eq!(count("edited") + count("reloaded"), 0, "{seen:?}");
}

#[gpui_kit::test]
fn edits_are_told_undone_and_done_again(cx: &mut TestAppContext) {
    init(cx);
    let buffer = cx.new(|_| Buffer::untitled_with(1, Document::new()));
    let seen = events(&buffer, cx);
    type_at(&buffer, 0, "héllo", cx);
    assert_eq!(text(&buffer, cx), "héllo");
    assert!(buffer.read_with(cx, |buffer, _| buffer.is_modified()));
    let told: Vec<&str> = seen
        .borrow()
        .iter()
        .copied()
        .filter(|event| matches!(*event, "edited" | "state"))
        .collect();
    assert_eq!(told, ["edited", "state"], "an edit, and modified");

    // A second edit is no change of state.
    type_at(&buffer, "héllo".len(), "!", cx);
    assert_eq!(text(&buffer, cx), "héllo!");
    assert_eq!(seen.borrow().iter().filter(|e| **e == "state").count(), 1);

    // Undo, step by step, down to the text it started with; then there is nothing to undo.
    let undone = buffer.update(cx, |buffer, cx| buffer.undo(None, cx));
    assert!(undone.is_some());
    assert_eq!(text(&buffer, cx), "héllo");
    buffer
        .update(cx, |buffer, cx| buffer.undo(None, cx))
        .unwrap();
    assert_eq!(text(&buffer, cx), "");
    assert!(!buffer.read_with(cx, |buffer, _| buffer.is_modified()));
    assert!(
        buffer
            .update(cx, |buffer, cx| buffer.undo(None, cx))
            .is_none()
    );
    assert_eq!(text(&buffer, cx), "");

    // Redo, and nothing past the last step.
    buffer
        .update(cx, |buffer, cx| buffer.redo(None, cx))
        .unwrap();
    buffer
        .update(cx, |buffer, cx| buffer.redo(None, cx))
        .unwrap();
    assert_eq!(text(&buffer, cx), "héllo!");
    assert!(
        buffer
            .update(cx, |buffer, cx| buffer.redo(None, cx))
            .is_none()
    );
}

#[gpui_kit::test]
fn the_format_changes_as_an_edit(cx: &mut TestAppContext) {
    init(cx);
    let buffer = cx.new(|_| Buffer::untitled_with(1, Document::new()));
    let seen = events(&buffer, cx);
    let format = buffer.read_with(cx, |buffer, _| buffer.doc().format());
    assert_eq!((format.encoding, format.bom), (Encoding::Utf8, false));

    // The format it has already: nothing happens.
    buffer.update(cx, |buffer, cx| {
        buffer.set_format(Encoding::Utf8, false, &Selection::point(0), cx);
    });
    assert!(seen.borrow().is_empty());
    assert!(!buffer.read_with(cx, |buffer, _| buffer.is_modified()));

    // A byte order mark: an edit that can be undone.
    buffer.update(cx, |buffer, cx| {
        buffer.set_format(Encoding::Utf8, true, &Selection::point(0), cx);
    });
    assert!(buffer.read_with(cx, |buffer, _| buffer.doc().format().bom));
    assert!(buffer.read_with(cx, |buffer, _| buffer.is_modified()));
    buffer
        .update(cx, |buffer, cx| buffer.undo(None, cx))
        .unwrap();
    assert!(!buffer.read_with(cx, |buffer, _| buffer.doc().format().bom));

    // An encoding that has no byte order mark takes none.
    let legacy = Encoding::Legacy("windows-1251");
    buffer.update(cx, |buffer, cx| {
        buffer.set_format(legacy, true, &Selection::point(0), cx);
    });
    let format = buffer.read_with(cx, |buffer, _| buffer.doc().format());
    assert_eq!((format.encoding, format.bom), (legacy, false));
}

#[gpui_kit::test]
fn documents_put_back_from_a_session(cx: &mut TestAppContext) {
    init(cx);
    let doc = || Document::from_text(Rope::from_str("kept"));
    // An untitled document with unsaved changes keeps its number.
    let untitled = cx.new(|cx| Buffer::restored(None, Some(2), doc(), true, false, cx));
    untitled.read_with(cx, |buffer, _| {
        assert_eq!(buffer.display_name(), "new 2");
        assert!(buffer.is_modified());
        assert_eq!(buffer.read_only(), None);
    });
    assert_eq!(text(&untitled, cx), "kept");

    // A file's document has no number, and is as it was saved unless it is said modified.
    let path = PathBuf::from("a.txt");
    let file = cx.new(|cx| Buffer::restored(Some(path), Some(7), doc(), false, true, cx));
    file.read_with(cx, |buffer, _| {
        assert_eq!(buffer.untitled_number(), None);
        assert_eq!(buffer.display_name(), "a.txt");
        assert!(!buffer.is_modified());
        assert_eq!(buffer.read_only(), Some(ReadOnly::Requested));
    });

    // Neither a file nor a number: the first untitled document.
    let neither = cx.new(|cx| Buffer::restored(None, None, Document::new(), false, false, cx));
    assert_eq!(
        neither.read_with(cx, |buffer, _| buffer.display_name()),
        "new 1"
    );
}

const UDL: &str = r#"<NotepadPlus>
    <UserLang name="Mini" ext="mini" udlVersion="2.1">
        <KeywordLists>
            <Keywords name="Keywords1">if else</Keywords>
        </KeywordLists>
    </UserLang>
</NotepadPlus>"#;

fn mini(name: &str, _: &App) -> Option<Rc<UserLanguage>> {
    let language = birchpad_syntax::parse_udl(UDL).unwrap().remove(0);
    language.matches_file(name).then(|| Rc::new(language))
}

#[gpui_kit::test]
fn files_take_the_user_defined_language_the_application_has_for_them(cx: &mut TestAppContext) {
    init(cx);
    // Without the application's languages, no file has one.
    assert!(
        cx.update(|cx| UserLanguages::for_file("a.mini", cx))
            .is_none()
    );
    let before = cx.new(|_| Buffer::untitled(1));
    before.update(cx, |buffer, cx| buffer.renamed("a.mini".into(), cx));
    assert_eq!(
        before.read_with(cx, |buffer, _| buffer.language_key()),
        "text"
    );

    cx.update(|cx| cx.set_global(UserLanguages(mini)));
    assert!(
        cx.update(|cx| UserLanguages::for_file("a.mini", cx))
            .is_some()
    );
    for other in ["a.txt", "mini", "a.", ""] {
        assert!(
            cx.update(|cx| UserLanguages::for_file(other, cx)).is_none(),
            "{other:?}"
        );
    }
    let buffer = cx.new(|_| Buffer::untitled(1));
    buffer.update(cx, |buffer, cx| buffer.renamed("a.mini".into(), cx));
    buffer.read_with(cx, |buffer, _| {
        assert_eq!(
            buffer
                .user_language()
                .map(|language| language.name.as_str()),
            Some("Mini")
        );
        assert_eq!(buffer.language_key(), "udl:Mini");
    });
    // A language Birchpad has itself comes first.
    buffer.update(cx, |buffer, cx| buffer.renamed("a.rs".into(), cx));
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.language_key()),
        "rust"
    );
}

#[gpui_kit::test]
fn files_are_read_in_the_background(cx: &mut TestAppContext) {
    init(cx);
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "first\r\nsecond").unwrap();
    let options = cx.update(|cx| AppState::global(cx).load_options(None));

    let buffer = cx.new(|cx| Buffer::open(path.clone(), options, false, cx));
    let seen = events(&buffer, cx);
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.read_only()),
        Some(ReadOnly::Loading),
        "not to be edited while it is read"
    );
    cx.run_until_parked();
    assert_eq!(text(&buffer, cx), "first\r\nsecond");
    buffer.read_with(cx, |buffer, _| {
        assert_eq!(buffer.read_only(), None);
        assert!(!buffer.is_modified());
        assert!(
            buffer.disk().is_some(),
            "how the file looked when it was read"
        );
        assert_eq!(buffer.loading_progress(), None);
    });
    assert!(seen.borrow().contains(&"reloaded"), "{:?}", seen.borrow());

    // An empty file is an empty document.
    let empty = dir.path().join("empty.txt");
    std::fs::write(&empty, "").unwrap();
    let buffer = cx.new(|cx| Buffer::open(empty, options, true, cx));
    cx.run_until_parked();
    assert_eq!(text(&buffer, cx), "");
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.read_only()),
        Some(ReadOnly::Requested)
    );

    // A file that is not there yet is an empty document with that path, made when it is saved.
    let path = dir.path().join("missing.txt");
    let missing = cx.new(|cx| Buffer::open(path.clone(), options, false, cx));
    let seen = events(&missing, cx);
    cx.run_until_parked();
    assert_eq!(text(&missing, cx), "");
    missing.read_with(cx, |buffer, _| {
        assert_eq!(buffer.path(), Some(path.as_path()));
        assert_eq!(buffer.read_only(), None);
        assert_eq!(buffer.disk(), None);
    });
    assert!(
        !seen.borrow().contains(&"load-failed"),
        "{:?}",
        seen.borrow()
    );
    assert!(!path.exists(), "nothing is written by opening");

    // What cannot be read (a folder) is said, and the buffer stays empty.
    let folder = cx.new(|cx| Buffer::open(dir.path().to_owned(), options, false, cx));
    let seen = events(&folder, cx);
    cx.run_until_parked();
    assert_eq!(text(&folder, cx), "");
    assert!(
        seen.borrow().contains(&"load-failed"),
        "{:?}",
        seen.borrow()
    );
}
