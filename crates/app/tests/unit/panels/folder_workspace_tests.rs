use std::fs;

use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext};

use super::*;
use crate::panels::PanelKind;
use crate::workspace::tests::{open_workspace, tab_names};

fn tree(paths: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for path in paths {
        let full = dir.path().join(path);
        if path.ends_with('/') {
            fs::create_dir_all(full).unwrap();
        } else {
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(full, "text").unwrap();
        }
    }
    dir
}

fn names(entries: &[Entry]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| format!("{}{}", entry.name, if entry.is_dir { "/" } else { "" }))
        .collect()
}

#[test]
fn folders_list_their_folders_first_in_name_order() {
    let dir = tree(&["b.txt", "A.txt", "zeta/", "Alpha/", "c.rs"]);
    assert_eq!(
        names(&list_folder(dir.path()).unwrap()),
        ["Alpha/", "zeta/", "A.txt", "b.txt", "c.rs"]
    );
    let empty = tree(&["empty/"]);
    assert!(list_folder(&empty.path().join("empty")).unwrap().is_empty());
    assert_eq!(
        list_folder(&dir.path().join("missing")).unwrap_err().kind(),
        io::ErrorKind::NotFound
    );
    assert!(list_folder(&dir.path().join("b.txt")).is_err(), "a file");
}

#[test]
fn a_new_listing_keeps_what_is_known_of_folders_still_there() {
    let dir = tree(&["keep/inner.txt", "gone/", "file.txt"]);
    let mut root = Node::root(dir.path().to_owned());
    root.set_listing(list_folder(dir.path()));
    let keep = root.find_mut(&dir.path().join("keep")).unwrap();
    keep.expanded = true;
    keep.set_listing(list_folder(&dir.path().join("keep")));

    fs::remove_dir(dir.path().join("gone")).unwrap();
    fs::write(dir.path().join("new.txt"), "").unwrap();
    root.expanded = true;
    root.set_listing(list_folder(dir.path()));
    let mut rows = Vec::new();
    push_rows(&root, 0, true, &mut rows);
    let shown: Vec<(usize, &str)> = rows
        .iter()
        .map(|row| (row.depth, row.name.as_str()))
        .collect();
    let top = root.name.clone();
    assert_eq!(
        shown,
        [
            (0, top.as_str()),
            (1, "keep"),
            (2, "inner.txt"),
            (1, "file.txt"),
            (1, "new.txt"),
        ],
        "keep stays unfolded with its contents"
    );

    // A folder that can no longer be read shows why, with nothing in it.
    fs::remove_dir_all(dir.path().join("keep")).unwrap();
    let keep = root.find_mut(&dir.path().join("keep")).unwrap();
    keep.set_listing(list_folder(&dir.path().join("keep")));
    assert!(keep.error.is_some());
    assert_eq!(keep.children.as_ref().map(Vec::len), Some(0));
    // Nothing is found outside the tree.
    assert!(root.find_mut(Path::new("/elsewhere")).is_none());
}

// --- In the window -------------------------------------------------------------------------

fn panel(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<FolderWorkspace> {
    workspace.read_with(cx, |workspace, _| {
        let view = workspace
            .docks
            .view(PanelKind::FolderWorkspace)
            .expect("open");
        view.clone()
            .downcast::<FolderWorkspace>()
            .expect("folder as workspace")
    })
}

fn rows(panel: &Entity<FolderWorkspace>, cx: &mut VisualTestContext) -> Vec<String> {
    panel.read_with(cx, |panel, _| {
        panel
            .rows()
            .iter()
            .map(|row| {
                let arrow = match (row.is_dir, row.expanded) {
                    (true, true) => "-",
                    (true, false) => "+",
                    _ => " ",
                };
                format!("{}{arrow}{}", "  ".repeat(row.depth), row.name)
            })
            .collect()
    })
}

fn add(workspace: &Entity<Workspace>, folder: &Path, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_workspace_folder(folder, window, cx);
    });
    cx.run_until_parked();
}

fn root_name(dir: &tempfile::TempDir) -> String {
    dir.path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

#[gpui_kit::test]
fn folders_unfold_and_files_open(cx: &mut TestAppContext) {
    let dir = tree(&["src/main.rs", "src/lib.rs", "README.md"]);
    let (workspace, cx) = open_workspace(cx);
    add(&workspace, dir.path(), cx);
    let panel = panel(&workspace, cx);
    let root = root_name(&dir);
    assert_eq!(
        rows(&panel, cx),
        [format!("-{root}"), "  +src".into(), "   README.md".into()]
    );

    // A click on a folder unfolds it, anywhere on its row, also right of a short name; a
    // double-click on a file opens it.
    let src = cx.debug_bounds("folder-row-1").expect("drawn");
    assert!(
        src.size.width > gpui_kit::px(150.),
        "the row spans the panel: {src:?}"
    );
    let right = gpui_kit::point(src.right() - gpui_kit::px(4.), src.center().y);
    cx.simulate_click(right, Modifiers::none());
    assert_eq!(
        rows(&panel, cx)[2..],
        ["     lib.rs", "     main.rs", "   README.md"]
    );
    let lib = cx.debug_bounds("folder-row-2").expect("drawn").center();
    cx.simulate_click(lib, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        tab_names(&workspace, cx),
        ["new 1"],
        "one click only selects a file"
    );
    cx.simulate_event(gpui_kit::MouseDownEvent {
        position: lib,
        button: gpui_kit::MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(gpui_kit::MouseUpEvent {
        position: lib,
        button: gpui_kit::MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 2,
    });
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx), ["lib.rs"]);

    // Fold All and Unfold All.
    panel.update(cx, |panel, cx| panel.set_all(false, cx));
    assert_eq!(rows(&panel, cx), [format!("+{root}")]);
    panel.update(cx, |panel, cx| panel.set_all(true, cx));
    assert_eq!(
        rows(&panel, cx).len(),
        5,
        "what had been read unfolds again"
    );
}

#[gpui_kit::test]
fn the_tree_follows_the_disk(cx: &mut TestAppContext) {
    let dir = tree(&["a.txt"]);
    let (workspace, cx) = open_workspace(cx);
    add(&workspace, dir.path(), cx);
    let panel = panel(&workspace, cx);
    fs::write(dir.path().join("b.txt"), "").unwrap();
    fs::remove_file(dir.path().join("a.txt")).unwrap();
    assert_eq!(rows(&panel, cx).len(), 2, "not yet");
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    assert_eq!(rows(&panel, cx)[1..], ["   b.txt"]);
    // A top folder that is gone says so.
    let gone = dir.path().to_owned();
    drop(dir);
    cx.executor().advance_clock(POLL);
    cx.run_until_parked();
    let error = panel.read_with(cx, |panel, _| panel.rows()[0].error.clone());
    assert!(error.is_some(), "{gone:?}");
}

#[gpui_kit::test]
fn the_keyboard_walks_the_tree(cx: &mut TestAppContext) {
    let dir = tree(&["sub/inner.txt", "top.txt"]);
    let (workspace, cx) = open_workspace(cx);
    add(&workspace, dir.path(), cx);
    let panel = panel(&workspace, cx);
    workspace.update_in(cx, |_, window, cx| {
        let focus = panel.read(cx).focus_handle.clone();
        window.focus(&focus, cx);
    });
    let selected = |cx: &mut VisualTestContext| {
        panel.read_with(cx, |panel, _| {
            panel
                .selected
                .as_ref()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
        })
    };
    // The added folder is selected; down to sub, right unfolds it, down into it.
    cx.simulate_keystrokes("down right down");
    assert_eq!(selected(cx).as_deref(), Some("inner.txt"));
    // Left goes up to the folder, and on an unfolded folder folds it.
    cx.simulate_keystrokes("left");
    assert_eq!(selected(cx).as_deref(), Some("sub"));
    cx.simulate_keystrokes("left");
    assert!(rows(&panel, cx)[1].contains("+sub"));
    // Enter opens a file; past the last row stays on it.
    cx.simulate_keystrokes("down down down enter");
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx), ["top.txt"]);
    // Keys with modifiers are not the tree's.
    cx.simulate_keystrokes("shift-up");
    assert_eq!(selected(cx).as_deref(), Some("top.txt"));
}

#[gpui_kit::test]
fn the_current_file_can_be_located(cx: &mut TestAppContext) {
    let dir = tree(&["a/b/deep.txt", "other.txt"]);
    let (workspace, cx) = open_workspace(cx);
    add(&workspace, dir.path(), cx);
    let panel = panel(&workspace, cx);
    let deep = dir.path().join("a").join("b").join("deep.txt");
    let located = panel.update(cx, |panel, cx| panel.locate(&deep, cx));
    assert!(located);
    assert_eq!(
        panel.read_with(cx, |panel, _| panel.selected.clone()),
        Some(deep)
    );
    assert!(rows(&panel, cx).iter().any(|row| row.ends_with("deep.txt")));
    // A file outside every top folder is not found.
    let elsewhere = tempfile::tempdir().unwrap();
    let outside = elsewhere.path().join("x.txt");
    assert!(!panel.update(cx, |panel, cx| panel.locate(&outside, cx)));
}

#[gpui_kit::test]
fn top_folders_are_remembered_added_once_and_removed(cx: &mut TestAppContext) {
    let one = tree(&["x.txt"]);
    let two = tree(&["y.txt"]);
    let (workspace, cx) = open_workspace(cx);
    add(&workspace, one.path(), cx);
    add(&workspace, two.path(), cx);
    add(&workspace, one.path(), cx);
    let panel = panel(&workspace, cx);
    let remembered = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| AppState::global(cx).state.panels.folders.clone())
    };
    assert_eq!(
        remembered(cx),
        [one.path().to_owned(), two.path().to_owned()]
    );
    assert_eq!(
        panel.read_with(cx, |panel, _| panel.roots()),
        remembered(cx)
    );

    panel.update(cx, |panel, cx| panel.remove_root(one.path(), cx));
    assert_eq!(remembered(cx), [two.path().to_owned()]);
    panel.update(cx, |panel, cx| {
        panel.remove_root(Path::new("/not/there"), cx)
    });
    assert_eq!(remembered(cx).len(), 1);
    panel.update(cx, |panel, cx| panel.remove_all(cx));
    assert!(remembered(cx).is_empty());
    assert!(rows(&panel, cx).is_empty());
}

#[gpui_kit::test]
fn remembered_folders_come_back_and_dropped_folders_are_added(cx: &mut TestAppContext) {
    let dir = tree(&["x.txt"]);
    let (workspace, cx) = open_workspace(cx);
    cx.update(|_, cx| {
        AppState::update_state(cx, |state, _| {
            state.panels.folders = vec![dir.path().to_owned()];
        });
    });
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_panel(PanelKind::FolderWorkspace, window, cx);
    });
    let panel = panel(&workspace, cx);
    assert_eq!(
        rows(&panel, cx),
        [format!("+{}", root_name(&dir))],
        "folded until opened"
    );

    // A folder dropped on the window, or named on the command line, is added.
    let dropped = tree(&["z.txt"]);
    cx.simulate_event(gpui_kit::FileDropEvent::Entered {
        position: gpui_kit::point(gpui_kit::px(400.), gpui_kit::px(300.)),
        paths: gpui_kit::ExternalPaths(vec![dropped.path().to_owned()].into()),
    });
    cx.simulate_event(gpui_kit::FileDropEvent::Submit {
        position: gpui_kit::point(gpui_kit::px(400.), gpui_kit::px(300.)),
    });
    cx.run_until_parked();
    let named = tree(&["w.txt"]);
    let command_line =
        birchpad_cli::CommandLine::parse([named.path().as_os_str().to_owned()], named.path());
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_command_line(&command_line, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        panel.read_with(cx, |panel, _| panel.roots()),
        [
            dir.path().to_owned(),
            dropped.path().to_owned(),
            named.path().to_owned()
        ]
    );
    assert_eq!(tab_names(&workspace, cx), ["new 1"], "no tab for a folder");
}
