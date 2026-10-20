//! Importing a Notepad++ `session.xml`: the files of both views with their carets, scroll
//! positions, bookmarks, collapsed folds, language and read-only flag, and unsaved text from
//! Notepad++'s backup copies, tab colors and pinned tabs. Encodings are detected again; the
//! other attributes (the document map) have no counterpart.

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
                pinned: file.attribute("tabPinned") == Some("yes"),
                color: tab_color(file.attribute("tabColourId")),
                rtl: file.attribute("RTL") == Some("yes"),
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

/// Notepad++ numbers its tab colors from 0 and writes -1 for none; Birchpad's go from 1 to 5.
fn tab_color(attribute: Option<&str>) -> Option<u8> {
    let id: u8 = attribute?.trim().parse().ok()?;
    (id < 5).then_some(id + 1)
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
