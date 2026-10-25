use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use birchpad::app_state::AppState;
use birchpad::finder::*;
use birchpad::quick_pick::QuickPick;
use birchpad::workspace::Workspace;
use birchpad_commands::Invocation;
use birchpad_config::FinderSettings;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use crate::workspace::{open_workspace, tab_names};

/// A folder `project` with `src/main.rs`, `src/lib.rs`, `README.md`, `.git/config` and
/// `target/out.txt`.
fn project() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    for (path, text) in [
        ("src/main.rs", "fn main() {}"),
        ("src/lib.rs", ""),
        ("README.md", "# Project"),
        (".git/config", ""),
        ("target/out.txt", ""),
    ] {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    (dir, root)
}

fn names(folders: &[(PathBuf, Vec<PathBuf>)]) -> Vec<String> {
    folders
        .iter()
        .flat_map(|(root, files)| files.iter().map(|path| shown_path(root, path)))
        .map(|text| text.replace('\\', "/"))
        .collect()
}

#[test]
fn files_are_shown_under_their_folder_s_name() {
    let root = Path::new("/home/me/project");
    let shown = shown_path(root, &root.join("src").join("main.rs")).replace('\\', "/");
    assert_eq!(shown, "project/src/main.rs");
    // A file outside the folder shows its whole path.
    let other = Path::new("/elsewhere/a.txt");
    assert_eq!(shown_path(root, other), other.display().to_string());
}

#[test]
fn folders_are_listed_without_version_control_and_build_folders() {
    let (_dir, root) = project();
    let folders = list_folders(
        std::slice::from_ref(&root),
        &FinderSettings::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let mut listed = names(&folders);
    listed.sort();
    assert_eq!(
        listed,
        [
            "project/README.md",
            "project/src/lib.rs",
            "project/src/main.rs"
        ]
    );
    // Its own filters: only Markdown.
    let settings = FinderSettings {
        filters: "*.md".into(),
        ..FinderSettings::default()
    };
    let folders = list_folders(&[root], &settings, &AtomicBool::new(false)).unwrap();
    assert_eq!(names(&folders), ["project/README.md"]);
}

#[test]
fn bad_filters_and_missing_folders_fail_cleanly() {
    let (dir, root) = project();
    let settings = FinderSettings {
        filters: "!+".into(),
        ..FinderSettings::default()
    };
    assert!(list_folders(&[root], &settings, &AtomicBool::new(false)).is_err());
    let missing = dir.path().join("missing");
    let folders = list_folders(
        &[missing],
        &FinderSettings::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(folders[0].1.is_empty());
    assert!(
        list_folders(&[], &FinderSettings::default(), &AtomicBool::new(false))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_cancelled_listing_stops() {
    let (_dir, root) = project();
    let folders =
        list_folders(&[root], &FinderSettings::default(), &AtomicBool::new(true)).unwrap();
    assert!(folders[0].1.is_empty());
}

#[test]
fn recent_files_come_first_once_and_the_limit_holds() {
    let (dir, root) = project();
    let main = root.join("src").join("main.rs");
    let gone = dir.path().join("gone.txt");
    let recent = recent_files(&[main.clone(), gone]);
    assert_eq!(recent.len(), 1, "a file that is gone is left out");
    let folders = list_folders(
        std::slice::from_ref(&root),
        &FinderSettings::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let all = merge(&recent, &folders, 100);
    assert_eq!(all[0].path, main);
    assert!(all[0].recent);
    assert_eq!(all.iter().filter(|found| found.path == main).count(), 1);
    assert_eq!(all.len(), 3);
    // The limit: the recent ones first.
    let two = merge(&recent, &folders, 2);
    assert_eq!(two.len(), 2);
    assert_eq!(two[0].path, main);
    assert!(merge(&recent, &folders, 0).is_empty());
    assert!(merge(&[], &[], 10).is_empty());
}

fn open_finder(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<QuickPick> {
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.dispatch(&Invocation::new("search.go-to-file"), window, cx)
        })
        .unwrap();
    cx.run_until_parked();
    birchpad::quick_pick::OPENED
        .with(|opened| opened.borrow().clone())
        .and_then(|pick| pick.upgrade())
        .expect("Go to File is open")
}

fn type_query(pick: &Entity<QuickPick>, text: &str, cx: &mut VisualTestContext) {
    let input = pick.read_with(cx, |pick, _| pick.input.clone());
    input.update_in(cx, |input, window, cx| input.set_value(text, window, cx));
    pick.update(cx, |pick, cx| pick.refilter(cx));
    cx.run_until_parked();
}

fn shown(pick: &Entity<QuickPick>, cx: &mut VisualTestContext) -> Vec<String> {
    pick.read_with(cx, |pick, _| {
        pick.shown
            .iter()
            .map(|(index, _)| pick.items()[*index].text.replace('\\', "/"))
            .collect()
    })
}

#[gpui_kit::test]
fn parts_of_a_path_find_a_workspace_file_and_enter_opens_it(cx: &mut TestAppContext) {
    let (dir, root) = project();
    let (workspace, cx) = open_workspace(cx);
    cx.update(|_, cx| {
        AppState::update_state(cx, |state, _| state.panels.folders = vec![root.clone()]);
    });
    let pick = open_finder(&workspace, cx);
    // The listing runs in the background.
    for _ in 0..50 {
        if shown(&pick, cx).len() == 3 {
            break;
        }
        cx.background_executor.run_until_parked();
        cx.run_until_parked();
    }
    assert_eq!(shown(&pick, cx).len(), 3);
    type_query(&pick, "src main", cx);
    assert_eq!(shown(&pick, cx), ["project/src/main.rs"]);
    pick.update_in(cx, |pick, window, cx| pick.pick(window, cx));
    cx.run_until_parked();
    // The untouched "new 1" gives way to the file.
    assert_eq!(tab_names(&workspace, cx), ["main.rs"]);
    drop(dir);
}

#[gpui_kit::test]
fn with_nothing_to_list_it_says_where_files_come_from(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let pick = open_finder(&workspace, cx);
    assert!(shown(&pick, cx).is_empty());
    let empty = pick.read_with(cx, |pick, _| pick.empty().to_owned());
    assert!(empty.contains("Open Folder as Workspace"), "{empty}");
    // Picking with nothing shown does nothing.
    pick.update_in(cx, |pick, window, cx| pick.pick(window, cx));
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx), ["new 1"]);
}

#[gpui_kit::test]
fn recent_files_are_listed_first_and_marked(cx: &mut TestAppContext) {
    let (dir, root) = project();
    let readme = root.join("README.md");
    let (workspace, cx) = open_workspace(cx);
    cx.update(|_, cx| {
        AppState::update_state(cx, |state, _| state.add_recent(&readme, 10));
    });
    let pick = open_finder(&workspace, cx);
    assert_eq!(
        shown(&pick, cx),
        [readme.display().to_string().replace('\\', "/")]
    );
    let hint = pick.read_with(cx, |pick, _| pick.items()[0].hint.clone());
    assert_eq!(hint, "recent");
    drop(dir);
}
