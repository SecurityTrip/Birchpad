//! Importing a Notepad++ `session.xml`: the files of both views with their carets, scroll
//! positions, bookmarks, collapsed folds, language and read-only flag, and unsaved text from
//! Notepad++'s backup copies. Encodings are detected again; Notepad++'s other attributes (tab
//! colors, the document map) have no counterpart.

use std::path::PathBuf;

use crate::session::{Session, SessionDocument, SessionError, SessionTab, SessionView};

/// Converts the text of a Notepad++ session file.
pub(crate) fn import(xml: &str) -> Result<Session, SessionError> {
    let document = roxmltree::Document::parse(xml)
        .map_err(|error| SessionError::Format(format!("not a Notepad++ session: {error}")))?;
    let session_node = document
        .descendants()
        .find(|node| node.has_tag_name("Session"))
        .ok_or_else(|| SessionError::Format("no <Session> in the file".into()))?;
    let mut session = Session::new();
    session.active_view = number(session_node.attribute("activeView")).min(1);
    for (index, tag) in ["mainView", "subView"].into_iter().enumerate() {
        let Some(view_node) = session_node.children().find(|node| node.has_tag_name(tag)) else {
            continue;
        };
        let mut view = SessionView {
            active: number(view_node.attribute("activeIndex")),
            tabs: Vec::new(),
        };
        for file in view_node
            .children()
            .filter(|node| node.has_tag_name("File"))
        {
            let Some(filename) = file.attribute("filename").filter(|name| !name.is_empty()) else {
                continue;
            };
            // A document in both views (a clone) is one document.
            let existing = session
                .documents
                .iter()
                .position(|document| is_same(document, filename));
            let document = match existing {
                Some(existing) => existing,
                None => {
                    session.documents.push(document_of(filename, file));
                    session.documents.len() - 1
                }
            };
            let lines = |tag: &str| -> Vec<usize> {
                file.children()
                    .filter(|node| node.has_tag_name(tag))
                    .map(|node| number(node.attribute("line")))
                    .collect()
            };
            if existing.is_none() {
                session.documents[document].bookmarks = lines("Mark");
            }
            view.tabs.push(SessionTab {
                document,
                selections: vec![[
                    number(file.attribute("startPos")),
                    number(file.attribute("endPos")),
                ]],
                primary: 0,
                first_row: number(file.attribute("firstVisibleLine")) as f64,
                scroll_x: number(file.attribute("xOffset")) as f32,
                folds: lines("Fold"),
            });
        }
        view.active = view.active.min(view.tabs.len().saturating_sub(1));
        session.views_mut()[index].clone_from(&view);
    }
    Ok(session)
}

fn document_of(filename: &str, file: roxmltree::Node) -> SessionDocument {
    let backup = file
        .attribute("backupFilePath")
        .filter(|path| !path.is_empty())
        .map(str::to_owned);
    let language = file
        .attribute("lang")
        .filter(|lang| !is_normal_text(lang))
        .map(str::to_owned);
    let mut document = SessionDocument {
        modified: backup.is_some(),
        backup,
        language,
        read_only: file.attribute("userReadOnly") == Some("yes"),
        ..SessionDocument::default()
    };
    match untitled_number(filename) {
        Some(number) => document.untitled = Some(number),
        None => document.path = Some(PathBuf::from(filename)),
    }
    document
}

fn is_same(document: &SessionDocument, filename: &str) -> bool {
    match untitled_number(filename) {
        Some(number) => document.untitled == Some(number),
        None => document.path.as_deref() == Some(PathBuf::from(filename).as_path()),
    }
}

/// "new 3" is Notepad++'s third untitled document.
fn untitled_number(filename: &str) -> Option<usize> {
    filename.strip_prefix("new ")?.parse().ok()
}

fn is_normal_text(lang: &str) -> bool {
    let lang = lang.to_ascii_lowercase();
    lang.is_empty() || lang.starts_with("normal") || lang.starts_with("none")
}

fn number(attribute: Option<&str>) -> usize {
    attribute
        .and_then(|value| value.trim().parse::<i64>().ok())
        .map_or(0, |value| usize::try_from(value).unwrap_or(0))
}

#[cfg(test)]
mod tests {
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
    fn rejects_other_xml() {
        assert!(import("<html></html>").is_err());
        assert!(import("not xml").is_err());
    }
}
