use std::fs;

use gpui_kit::component::WindowExt as _;
use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext};

use crate::workspace::{open_workspace, secondary, tab_names};
use birchpad::app_state::AppState;
use birchpad::file_commands::{SHELL, Shell};
use birchpad::panels::PanelKind;
use birchpad::panels::folder_workspace::*;
use birchpad::workspace::Workspace;
use std::io;
use std::path::{Path, PathBuf};

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

// --- Working with files ----------------------------------------------------------------

fn open_file(workspace: &Entity<Workspace>, path: &Path, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_path(path, window, cx)
    });
    cx.run_until_parked();
}

/// The files of the open documents, in tab order.
fn open_paths(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Vec<Option<PathBuf>> {
    workspace.read_with(cx, |workspace, cx| {
        workspace
            .all_views(cx)
            .iter()
            .map(|view| view.read(cx).buffer.read(cx).path().map(Path::to_owned))
            .collect()
    })
}

fn focus(panel: &Entity<FolderWorkspace>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        let focus = panel.read(cx).focus_handle.clone();
        window.focus(&focus, cx);
    });
}

#[gpui_kit::test]
fn new_files_and_folders_in_the_tree(cx: &mut TestAppContext) {
    let dir = tree(&["src/taken.txt"]);
    let (workspace, cx) = open_workspace(cx);
    add(&workspace, dir.path(), cx);
    let panel = panel(&workspace, cx);
    let src = dir.path().join("src");

    // A new file in a folder not unfolded yet: the folder unfolds, the file is selected and
    // opens in the editor.
    let made = panel
        .update_in(cx, |panel, window, cx| {
            panel.new_file(&src, " new.txt ", window, cx)
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(made, src.join("new.txt"), "the name without its spaces");
    assert!(made.is_file());
    assert!(rows(&panel, cx).contains(&"  -src".to_owned()));
    assert!(rows(&panel, cx).contains(&"     new.txt".to_owned()));
    assert_eq!(
        panel.read_with(cx, |panel, _| panel.selected.clone()),
        Some(made)
    );
    assert_eq!(tab_names(&workspace, cx), ["new.txt"]);

    let folder = panel
        .update(cx, |panel, cx| panel.new_folder(dir.path(), "made", cx))
        .unwrap();
    assert!(folder.is_dir());
    assert!(rows(&panel, cx).contains(&"  +made".to_owned()));

    // A name taken, or no name: refused, and nothing changes.
    let before = rows(&panel, cx);
    let taken = panel.update_in(cx, |panel, window, cx| {
        panel.new_file(&src, "taken.txt", window, cx)
    });
    assert!(taken.unwrap_err().to_string().contains("already exists"));
    assert_eq!(fs::read_to_string(src.join("taken.txt")).unwrap(), "text");
    assert!(
        panel
            .update(cx, |panel, cx| panel.new_folder(&src, "", cx))
            .is_err()
    );
    assert!(
        panel
            .update(cx, |panel, cx| panel.new_folder(&src, "a/b", cx))
            .is_err()
    );
    assert_eq!(rows(&panel, cx), before);
    assert_eq!(tab_names(&workspace, cx), ["new.txt"]);

    // New File... asks for the name in a dialog.
    panel.update_in(cx, |panel, window, cx| panel.ask_new_file(&src, window, cx));
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
}

#[gpui_kit::test]
fn renamed_files_and_folders_take_their_documents_along(cx: &mut TestAppContext) {
    let dir = tree(&["src/a.txt", "src/deep/b.txt", "other.txt"]);
    let (workspace, cx) = open_workspace(cx);
    add(&workspace, dir.path(), cx);
    let panel = panel(&workspace, cx);
    let src = dir.path().join("src");
    open_file(&workspace, &src.join("a.txt"), cx);
    open_file(&workspace, &src.join("deep").join("b.txt"), cx);
    cx.simulate_input("unsaved ");
    panel.update(cx, |panel, cx| {
        panel.expand(&src, cx);
        panel.expand(&src.join("deep"), cx);
        panel.selected = Some(src.join("deep").join("b.txt"));
    });

    // The folder: its documents follow, unsaved changes and all, and it stays unfolded.
    let lib = panel
        .update(cx, |panel, cx| panel.rename_entry(&src, "lib", cx))
        .unwrap();
    cx.run_until_parked();
    assert_eq!(lib, dir.path().join("lib"));
    assert_eq!(
        open_paths(&workspace, cx),
        [
            Some(lib.join("a.txt")),
            Some(lib.join("deep").join("b.txt"))
        ]
    );
    assert_eq!(tab_names(&workspace, cx), ["a.txt", "b.txt"]);
    let root = root_name(&dir);
    assert_eq!(
        rows(&panel, cx),
        [
            format!("-{root}"),
            "  -lib".into(),
            "    -deep".into(),
            "       b.txt".into(),
            "     a.txt".into(),
            "   other.txt".into(),
        ]
    );
    assert_eq!(
        panel.read_with(cx, |panel, _| panel.selected.clone()),
        Some(lib.join("deep").join("b.txt")),
        "the selection follows"
    );

    // A file, by the name of its tab too.
    panel
        .update(cx, |panel, cx| {
            panel.rename_entry(&lib.join("a.txt"), "c.md", cx)
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx), ["c.md", "b.txt"]);

    // A name with a folder in it, or one taken, is refused; the files stay as they were.
    let refused = panel.update(cx, |panel, cx| {
        panel.rename_entry(&lib.join("c.md"), "../other.txt", cx)
    });
    assert!(refused.is_err());
    let refused = panel.update(cx, |panel, cx| panel.rename_entry(&lib, "other.txt", cx));
    assert!(refused.is_err());
    assert_eq!(tab_names(&workspace, cx), ["c.md", "b.txt"]);
    // The same name changes nothing.
    assert_eq!(
        panel
            .update(cx, |panel, cx| panel.rename_entry(&lib, "lib", cx))
            .unwrap(),
        lib
    );

    // A top folder is renamed, and remembered by its new name.
    let renamed = panel
        .update(cx, |panel, cx| {
            panel.rename_entry(dir.path(), "renamed-root", cx)
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        panel.read_with(cx, |panel, _| panel.roots()),
        [renamed.clone()]
    );
    assert_eq!(
        cx.update(|_, cx| AppState::global(cx).state.panels.folders.clone()),
        [renamed.clone()]
    );
    assert_eq!(rows(&panel, cx)[0], "-renamed-root");
    assert_eq!(
        open_paths(&workspace, cx)[0],
        Some(renamed.join("lib").join("c.md"))
    );
    // So that the temporary folder is removed.
    fs::rename(&renamed, dir.path()).unwrap();
}

#[gpui_kit::test]
fn copy_cut_paste_and_duplicate(cx: &mut TestAppContext) {
    let dir = tree(&["a.txt", "to/", "src/main.rs"]);
    let (workspace, cx) = open_workspace(cx);
    add(&workspace, dir.path(), cx);
    let panel = panel(&workspace, cx);
    let (a, to) = (dir.path().join("a.txt"), dir.path().join("to"));

    // Nothing set aside yet.
    let nothing = panel.update(cx, |panel, cx| panel.paste_into(&to, cx));
    assert!(
        nothing
            .unwrap_err()
            .to_string()
            .contains("nothing to paste")
    );

    // Copy and Paste: into the folder, and onto a file, into the file's folder.
    panel.update(cx, |panel, _| panel.copy_entry(&a));
    let pasted = panel
        .update(cx, |panel, cx| panel.paste_into(&to, cx))
        .unwrap();
    assert_eq!(pasted, to.join("a.txt"));
    let again = panel
        .update(cx, |panel, cx| panel.paste_into(&pasted, cx))
        .unwrap();
    assert_eq!(again, to.join("a copy.txt"), "a name of its own");
    assert!(a.exists(), "a copy leaves the original");
    assert!(
        panel.read_with(cx, |panel, _| panel.clipboard.is_some()),
        "it can be pasted again"
    );

    // A folder copied into itself is refused.
    panel.update(cx, |panel, _| panel.copy_entry(&dir.path().join("src")));
    let into_itself = panel.update(cx, |panel, cx| {
        panel.paste_into(&dir.path().join("src").join("main.rs"), cx)
    });
    assert!(into_itself.is_err());

    // Cut and Paste moves it, its document along, once.
    open_file(&workspace, &a, cx);
    panel.update(cx, |panel, _| panel.cut_entry(&a)).unwrap();
    let moved = panel
        .update(cx, |panel, cx| {
            panel.paste_into(&dir.path().join("src"), cx)
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(moved, dir.path().join("src").join("a.txt"));
    assert!(!a.exists());
    assert_eq!(open_paths(&workspace, cx), [Some(moved.clone())]);
    assert!(panel.read_with(cx, |panel, _| panel.clipboard.is_none()));

    // Duplicate: beside itself.
    let copy = panel
        .update(cx, |panel, cx| panel.duplicate_entry(&moved, cx))
        .unwrap();
    assert_eq!(copy, dir.path().join("src").join("a copy.txt"));
    assert_eq!(
        panel.read_with(cx, |panel, _| panel.selected.clone()),
        Some(copy)
    );

    // A top folder is not cut or duplicated, but it is copied.
    assert!(
        panel
            .update(cx, |panel, _| panel.cut_entry(dir.path()))
            .is_err()
    );
    assert!(
        panel
            .update(cx, |panel, cx| panel.duplicate_entry(dir.path(), cx))
            .is_err()
    );
    panel.update(cx, |panel, _| panel.copy_entry(dir.path()));
    assert_eq!(
        panel.read_with(cx, |panel, _| panel.clipboard.clone()),
        Some(Clip {
            path: dir.path().to_owned(),
            cut: false
        })
    );
}

#[gpui_kit::test]
fn the_keys_of_the_tree_work_with_files(cx: &mut TestAppContext) {
    let dir = tree(&["a.txt", "to/"]);
    let (workspace, cx) = open_workspace(cx);
    add(&workspace, dir.path(), cx);
    let panel = panel(&workspace, cx);
    focus(&panel, cx);
    let a = dir.path().join("a.txt");
    panel.update(cx, |panel, _| panel.selected = Some(a.clone()));

    // Ctrl+C and Ctrl+V: a copy beside it; Ctrl+D: another.
    cx.simulate_keystrokes(&format!("{} {}", secondary("c"), secondary("v")));
    assert!(dir.path().join("a copy.txt").exists());
    panel.update(cx, |panel, _| panel.selected = Some(a.clone()));
    cx.simulate_keystrokes(&secondary("d"));
    assert!(dir.path().join("a copy 2.txt").exists());
    assert_eq!(
        tab_names(&workspace, cx),
        ["new 1"],
        "the editor's keys did nothing"
    );

    // Ctrl+X, then Ctrl+V on the folder: moved.
    panel.update(cx, |panel, _| panel.selected = Some(a.clone()));
    cx.simulate_keystrokes(&secondary("x"));
    panel.update(cx, |panel, _| panel.selected = Some(dir.path().join("to")));
    cx.simulate_keystrokes(&secondary("v"));
    assert!(dir.path().join("to").join("a.txt").exists() && !a.exists());

    // F2 asks for the new name.
    cx.simulate_keystrokes("f2");
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    cx.update(|window, cx| window.close_dialog(cx));

    // Delete asks first; Cancel keeps it.
    panel.update(cx, |panel, _| {
        panel.selected = Some(dir.path().join("a copy.txt"))
    });
    focus(&panel, cx);
    cx.simulate_keystrokes("delete");
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert!(dir.path().join("a copy.txt").exists());

    // Nothing selected: the keys do nothing.
    panel.update(cx, |panel, _| panel.selected = None);
    cx.simulate_keystrokes(&format!("{} delete", secondary("d")));
    assert!(!cx.has_pending_prompt());
}

#[gpui_kit::test]
fn moving_to_the_recycle_bin_from_the_tree(cx: &mut TestAppContext) {
    let dir = tree(&[
        "keep.txt",
        "gone.txt",
        "folder/inner.txt",
        "folder/changed.txt",
    ]);
    let (workspace, cx) = open_workspace(cx);
    add(&workspace, dir.path(), cx);
    let panel = panel(&workspace, cx);
    let folder = dir.path().join("folder");
    open_file(&workspace, &dir.path().join("gone.txt"), cx);
    open_file(&workspace, &folder.join("inner.txt"), cx);
    open_file(&workspace, &folder.join("changed.txt"), cx);
    cx.simulate_input("unsaved ");

    // Asked first: Move.
    panel
        .update_in(cx, |panel, window, cx| {
            panel.delete_entry(&dir.path().join("gone.txt"), window, cx)
        })
        .unwrap();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Move");
    cx.run_until_parked();
    assert!(!dir.path().join("gone.txt").exists());
    assert_eq!(tab_names(&workspace, cx), ["inner.txt", "changed.txt"]);

    // Without asking, a folder: the tab with unsaved changes stays.
    cx.update(|_, cx| {
        cx.global_mut::<AppState>()
            .settings
            .misc
            .confirm_workspace_delete = false;
    });
    panel.update(cx, |panel, cx| {
        panel.copy_entry(&folder.join("inner.txt"));
        panel.selected = Some(folder.join("inner.txt"));
        panel.expand(&folder, cx);
    });
    panel
        .update_in(cx, |panel, window, cx| {
            panel.delete_entry(&folder, window, cx)
        })
        .unwrap();
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    assert!(!folder.exists());
    assert_eq!(tab_names(&workspace, cx), ["changed.txt"]);
    let root = root_name(&dir);
    assert_eq!(rows(&panel, cx), [format!("-{root}"), "   keep.txt".into()]);
    assert_eq!(
        panel.read_with(cx, |panel, _| (
            panel.selected.clone(),
            panel.clipboard.clone()
        )),
        (Some(dir.path().to_owned()), None),
        "what was in it is no longer selected or set aside"
    );

    // A file already gone, and a top folder: refused.
    let gone = panel.update_in(cx, |panel, window, cx| {
        panel.delete_entry(&dir.path().join("gone.txt"), window, cx)
    });
    assert!(gone.is_err());
    let top = panel.update_in(cx, |panel, window, cx| {
        panel.delete_entry(dir.path(), window, cx)
    });
    assert!(top.unwrap_err().to_string().contains("top folder"));
    assert!(dir.path().join("keep.txt").exists());
}

#[gpui_kit::test]
fn dragging_moves_onto_folders(cx: &mut TestAppContext) {
    let dir = tree(&["a.txt", "to/taken.txt", "taken.txt"]);
    let (workspace, cx) = open_workspace(cx);
    add(&workspace, dir.path(), cx);
    let panel = panel(&workspace, cx);
    let root = root_name(&dir);
    assert_eq!(
        rows(&panel, cx),
        [
            format!("-{root}"),
            "  +to".into(),
            "   a.txt".into(),
            "   taken.txt".into()
        ]
    );

    // a.txt dragged onto the folder `to`.
    let from = cx.debug_bounds("folder-row-2").expect("drawn").center();
    let onto = cx.debug_bounds("folder-row-1").expect("drawn").center();
    cx.simulate_mouse_down(from, gpui_kit::MouseButton::Left, Modifiers::none());
    for step in 1..=4 {
        let t = step as f32 / 4.;
        let at = gpui_kit::point(
            from.x + (onto.x - from.x) * t,
            from.y + (onto.y - from.y) * t,
        );
        cx.simulate_mouse_move(at, gpui_kit::MouseButton::Left, Modifiers::none());
    }
    cx.simulate_mouse_up(onto, gpui_kit::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let to = dir.path().join("to");
    assert!(to.join("a.txt").exists() && !dir.path().join("a.txt").exists());
    assert!(
        rows(&panel, cx).contains(&"  -to".to_owned()),
        "unfolded where it went"
    );

    // Dropped on a file: into that file's folder; on itself or where it is: nothing.
    panel.update_in(cx, |panel, window, cx| {
        let dragged = DraggedEntry::new(to.join("a.txt"));
        panel.drop_on(&dragged, &dir.path().join("taken.txt"), window, cx);
    });
    assert!(dir.path().join("a.txt").exists());
    panel.update_in(cx, |panel, window, cx| {
        let dragged = DraggedEntry::new(to.clone());
        panel.drop_on(&dragged, &to.join("taken.txt"), window, cx);
        panel.drop_on(&dragged, &dir.path().join("a.txt"), window, cx);
    });
    assert!(to.is_dir() && to.join("taken.txt").exists());

    // A name taken there: refused, and both stay.
    let taken = panel.update(cx, |panel, cx| {
        panel.move_entry(&dir.path().join("taken.txt"), &to, cx)
    });
    assert!(taken.is_err());
    assert_eq!(fs::read_to_string(to.join("taken.txt")).unwrap(), "text");
    // A top folder stays where it is.
    let other = tree(&["x/"]);
    let top = panel.update(cx, |panel, cx| {
        panel.move_entry(dir.path(), &other.path().join("x"), cx)
    });
    assert!(top.is_err());
    assert!(dir.path().join("taken.txt").exists());
}

#[gpui_kit::test]
fn the_file_manager_shows_the_entry_itself(cx: &mut TestAppContext) {
    let dir = tree(&["a.txt"]);
    let (workspace, cx) = open_workspace(cx);
    add(&workspace, dir.path(), cx);
    let panel = panel(&workspace, cx);
    SHELL.with(|shell| shell.borrow_mut().clear());
    panel.update_in(cx, |panel, window, cx| {
        panel.reveal(&dir.path().join("a.txt"), window, cx)
    });
    assert_eq!(
        SHELL.with(|shell| std::mem::take(&mut *shell.borrow_mut())),
        [Shell::Reveal(dir.path().join("a.txt"))]
    );
}
