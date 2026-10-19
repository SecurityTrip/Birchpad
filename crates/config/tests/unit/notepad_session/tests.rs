use super::*;

const SESSION: &str = r#"<?xml version="1.0" encoding="UTF-8" ?>
<NotepadPlus>
    <Session activeView="1">
        <mainView activeIndex="1">
            <File firstVisibleLine="12" xOffset="40" startPos="5" endPos="9" lang="Normal text" encoding="-1" userReadOnly="no" filename="C:\notes\a.txt" backupFilePath="">
                <Mark line="2" />
                <Mark line="8" />
                <Fold line="4" />
            </File>
            <File firstVisibleLine="0" xOffset="0" startPos="3" endPos="3" lang="Python" encoding="-1" userReadOnly="yes" filename="new 2" backupFilePath="C:\Users\me\AppData\Roaming\Notepad++\backup\new 2@2024-01-01_120000" />
        </mainView>
        <subView activeIndex="0">
            <File firstVisibleLine="1" xOffset="0" startPos="0" endPos="0" lang="Normal text" filename="C:\notes\a.txt" backupFilePath="" />
        </subView>
    </Session>
</NotepadPlus>
"#;

#[test]
fn imports_files_views_and_backups() {
    let session = import(SESSION).unwrap();
    assert_eq!(session.active_view, 1);
    assert_eq!(session.documents.len(), 2, "the clone is the same document");
    let [notes, untitled] = [&session.documents[0], &session.documents[1]];
    assert_eq!(notes.path, Some(PathBuf::from(r"C:\notes\a.txt")));
    assert_eq!(
        (notes.bookmarks.as_slice(), notes.modified),
        (&[2, 8][..], false)
    );
    assert_eq!(notes.language, None);
    assert_eq!(untitled.untitled, Some(2));
    assert!(untitled.modified && untitled.read_only);
    assert_eq!(untitled.language.as_deref(), Some("Python"));
    assert!(
        untitled
            .backup
            .as_deref()
            .unwrap()
            .ends_with("new 2@2024-01-01_120000")
    );

    let main = &session.main_view;
    assert_eq!(main.active, 1);
    assert_eq!(main.tabs[0].selections, [[5, 9]]);
    assert_eq!((main.tabs[0].first_row, main.tabs[0].scroll_x), (12., 40.));
    assert_eq!(main.tabs[0].folds, [4]);
    assert_eq!(session.second_view.tabs[0].document, 0);
}

#[test]
fn a_session_with_one_view_and_odd_entries() {
    // No subView, an activeView past the second view, a File without a name or with an
    // empty one, and numbers that are not numbers.
    let xml = r#"<NotepadPlus><Session activeView="7">
            <mainView activeIndex="x">
                <File startPos="1" endPos="2" />
                <File filename="" />
                <File filename="C:\b.txt" startPos="-3" firstVisibleLine="many" />
            </mainView>
        </Session></NotepadPlus>"#;
    let session = import(xml).unwrap();
    assert_eq!(session.active_view, 1, "at most the second view");
    assert_eq!(session.documents.len(), 1);
    assert_eq!(session.main_view.tabs.len(), 1);
    assert_eq!(session.main_view.active, 0);
    assert_eq!(session.main_view.tabs[0].selections, [[0, 0]]);
    assert!(session.second_view.tabs.is_empty());
}

#[test]
fn tab_colors_and_pinned_tabs() {
    let xml = r#"<NotepadPlus><Session>
            <mainView>
                <File filename="C:\a.txt" tabColourId="0" tabPinned="yes" />
                <File filename="C:\b.txt" tabColourId="4" tabPinned="no" />
                <File filename="C:\c.txt" tabColourId="-1" />
                <File filename="C:\d.txt" tabColourId="5" />
                <File filename="C:\e.txt" tabColourId="red" tabPinned="YES" />
            </mainView>
        </Session></NotepadPlus>"#;
    let session = import(xml).unwrap();
    let tabs: Vec<(bool, Option<u8>)> = session
        .main_view
        .tabs
        .iter()
        .map(|tab| (tab.pinned, tab.color))
        .collect();
    assert_eq!(
        tabs,
        [
            (true, Some(1)),
            (false, Some(5)),
            (false, None),
            (false, None),
            (false, None)
        ],
        "colors 0 to 4 become 1 to 5; none, past the last or not a number, no color"
    );
}

#[test]
fn the_text_direction_of_each_tab() {
    let xml = r#"<NotepadPlus><Session><mainView>
            <File filename="C:\a.txt" RTL="yes" />
            <File filename="C:\b.txt" RTL="no" />
            <File filename="C:\c.txt" />
        </mainView></Session></NotepadPlus>"#;
    let session = import(xml).unwrap();
    let rtl: Vec<bool> = session.main_view.tabs.iter().map(|tab| tab.rtl).collect();
    assert_eq!(rtl, [true, false, false]);
}

#[test]
fn an_empty_session_has_no_documents() {
    let session = import("<NotepadPlus><Session /></NotepadPlus>").unwrap();
    assert!(session.documents.is_empty());
    assert_eq!(session.active_view, 0);
}

#[test]
fn rejects_other_xml() {
    assert!(import("<html></html>").is_err());
    assert!(import("not xml").is_err());
}
