use std::fs;

use gpui_kit::component::WindowExt as _;
use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext};

use super::*;
use crate::panels::PanelKind;
use crate::workspace::tests::{open_workspace, secondary, tab_names};

fn file(name: &str) -> ProjectItem {
    ProjectItem::File(PathBuf::from(name))
}

/// Project "P" with folder "F" (a.txt) and b.txt; project "Q", empty.
fn model() -> ProjectWorkspace {
    ProjectWorkspace {
        projects: vec![
            ProjectFolder {
                name: "P".into(),
                items: vec![
                    ProjectItem::Folder(ProjectFolder {
                        name: "F".into(),
                        items: vec![file("a.txt")],
                    }),
                    file("b.txt"),
                ],
            },
            ProjectFolder {
                name: "Q".into(),
                items: vec![],
            },
        ],
    }
}

#[test]
fn addresses_lead_to_projects_and_folders_only() {
    let mut model = model();
    assert_eq!(
        folder_mut(&mut model, &[0]).map(|f| f.name.clone()),
        Some("P".into())
    );
    assert_eq!(
        folder_mut(&mut model, &[0, 0]).map(|f| f.name.clone()),
        Some("F".into())
    );
    assert_eq!(
        folder_mut(&mut model, &[1]).map(|f| f.name.clone()),
        Some("Q".into())
    );
    for wrong in [&[][..], &[2], &[0, 1], &[0, 0, 0], &[0, 5]] {
        assert!(folder_mut(&mut model, wrong).is_none(), "{wrong:?}");
    }
}

#[test]
fn folders_and_files_are_added_where_asked() {
    let mut model = model();
    assert_eq!(add_folder(&mut model, &[1], "New"), Some(vec![1, 0]));
    assert_eq!(
        add_folder(&mut model, &[0, 0], "Inner"),
        Some(vec![0, 0, 1])
    );
    assert_eq!(add_folder(&mut model, &[0, 1], "On a file"), None);
    assert!(add_files(
        &mut model,
        &[1, 0],
        vec!["c.txt".into(), "c.txt".into()]
    ));
    // A file already in the folder is not added twice.
    assert!(add_files(
        &mut model,
        &[1, 0],
        vec!["c.txt".into(), "d.txt".into()]
    ));
    let new = folder_mut(&mut model, &[1, 0]).unwrap();
    assert_eq!(new.items, [file("c.txt"), file("d.txt")]);
    assert!(!add_files(&mut model, &[9], vec!["x".into()]));
    assert!(
        add_files(&mut model, &[1], Vec::new()),
        "nothing to add is fine"
    );
}

#[test]
fn rename_modify_and_remove() {
    let mut model = model();
    assert!(rename(&mut model, &[0], "Renamed"));
    assert!(rename(&mut model, &[0, 0], "Folder"));
    assert!(!rename(
        &mut model,
        &[0, 1],
        "a file has no name of its own"
    ));
    assert!(!rename(&mut model, &[7], "nothing"));
    assert!(set_file_path(&mut model, &[0, 1], "z.txt".into()));
    assert!(!set_file_path(&mut model, &[0, 0], "folder".into()));
    assert!(!set_file_path(&mut model, &[0], "project".into()));
    assert!(!set_file_path(&mut model, &[0, 9], "past the end".into()));
    assert_eq!(model.projects[0].name, "Renamed");
    assert_eq!(model.projects[0].items[1], file("z.txt"));

    assert!(remove(&mut model, &[0, 0, 0]), "a file in a folder");
    assert!(remove(&mut model, &[0, 0]), "a folder");
    assert!(remove(&mut model, &[1]), "a project");
    assert!(!remove(&mut model, &[1]));
    assert!(!remove(&mut model, &[0, 3]));
    assert!(!remove(&mut model, &[]));
    assert_eq!(model.projects.len(), 1);
    assert_eq!(model.projects[0].items, [file("z.txt")]);
}

#[test]
fn items_move_among_their_siblings() {
    let mut model = model();
    assert_eq!(move_item(&mut model, &[0, 1], true), Some(vec![0, 0]));
    assert_eq!(model.projects[0].items[0], file("b.txt"));
    assert_eq!(move_item(&mut model, &[0, 0], true), None, "already first");
    assert_eq!(move_item(&mut model, &[0, 1], false), None, "already last");
    assert_eq!(
        move_item(&mut model, &[1], true),
        Some(vec![0]),
        "projects too"
    );
    assert_eq!(model.projects[0].name, "Q");
    assert_eq!(move_item(&mut model, &[5], false), None);
    assert_eq!(move_item(&mut model, &[0, 9], false), None);
    assert_eq!(move_item(&mut model, &[], false), None);
}

#[test]
fn a_directory_brings_its_folders_and_files() {
    let dir = tempfile::tempdir().unwrap();
    for path in ["b.txt", "A.txt", "sub/c.txt", ".git/config", ".hidden"] {
        let full = dir.path().join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, "").unwrap();
    }
    assert_eq!(
        items_from_directory(dir.path()),
        [
            ProjectItem::Folder(ProjectFolder {
                name: "sub".into(),
                items: vec![ProjectItem::File(dir.path().join("sub").join("c.txt"))],
            }),
            ProjectItem::File(dir.path().join("A.txt")),
            ProjectItem::File(dir.path().join("b.txt")),
        ],
        "folders first, hidden entries left out"
    );
    assert!(items_from_directory(&dir.path().join("missing")).is_empty());
}

// --- In the window -------------------------------------------------------------------------

fn panel(
    workspace: &Entity<Workspace>,
    number: u8,
    cx: &mut VisualTestContext,
) -> Entity<ProjectPanel> {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.open_panel(PanelKind::Project(number), window, cx);
        let view = workspace.docks.view(PanelKind::Project(number)).unwrap();
        view.clone().downcast::<ProjectPanel>().unwrap()
    })
}

fn rows(panel: &Entity<ProjectPanel>, cx: &mut VisualTestContext) -> Vec<String> {
    panel.read_with(cx, |panel, _| {
        panel
            .rows()
            .iter()
            .map(|row| {
                let missing = matches!(row.kind, RowKind::File { exists: false, .. });
                format!(
                    "{}{}{}",
                    "  ".repeat(row.depth),
                    row.name,
                    if missing { " (missing)" } else { "" }
                )
            })
            .collect()
    })
}

#[gpui_kit::test]
fn a_new_workspace_is_built_and_saved_at_once(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("my.workspace");
    let source = dir.path().join("main.rs");
    fs::write(&source, "fn main() {}").unwrap();
    let (workspace, cx) = open_workspace(cx);
    let panel = panel(&workspace, 1, cx);
    assert!(rows(&panel, cx).is_empty(), "no workspace yet");

    panel.update(cx, |panel, cx| {
        panel.new_workspace(path.clone(), cx);
        panel.add_project("Editor", cx);
        panel.change(cx, |model| add_folder(model, &[0], "Sources").map(Some));
        let gone = dir.path().join("gone.txt");
        panel.change(cx, |model| {
            add_files(model, &[0, 0], vec![source.clone(), gone]).then_some(None)
        });
    });
    assert_eq!(
        rows(&panel, cx),
        [
            "my.workspace",
            "  Editor",
            "    Sources",
            "      main.rs",
            "      gone.txt (missing)"
        ]
    );
    let saved = ProjectWorkspace::load(&path).unwrap();
    assert_eq!(saved, panel.read_with(cx, |panel, _| panel.model.clone()));
    let remembered = cx.update(|_, cx| AppState::global(cx).state.panels.projects.clone());
    assert_eq!(remembered.get("1"), Some(&path));

    // A double-click opens a file; a click on a project folds it.
    let main = cx.debug_bounds("project-row-3").expect("drawn").center();
    cx.simulate_event(gpui_kit::MouseDownEvent {
        position: main,
        button: gpui_kit::MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(gpui_kit::MouseUpEvent {
        position: main,
        button: gpui_kit::MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 2,
    });
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx), ["main.rs"]);
    let project = cx.debug_bounds("project-row-1").expect("drawn").center();
    cx.simulate_click(project, Modifiers::none());
    assert_eq!(rows(&panel, cx), ["my.workspace", "  Editor"]);
}

#[gpui_kit::test]
fn the_keyboard_selects_opens_and_removes(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("k.workspace");
    let (workspace, cx) = open_workspace(cx);
    let panel = panel(&workspace, 2, cx);
    panel.update(cx, |panel, cx| {
        panel.new_workspace(path.clone(), cx);
        panel.add_project("One", cx);
        panel.add_project("Two", cx);
    });
    workspace.update_in(cx, |_, window, cx| {
        let focus = panel.read(cx).focus_handle.clone();
        window.focus(&focus, cx);
    });
    // From Two (added last, selected) up to One, and Delete removes it.
    cx.simulate_keystrokes("up delete");
    assert_eq!(rows(&panel, cx), ["k.workspace", "  Two"]);
    assert_eq!(
        ProjectWorkspace::load(&path).unwrap().projects.len(),
        1,
        "saved"
    );
    // Delete on the workspace row removes nothing; Enter on a project folds it.
    cx.simulate_keystrokes("up up delete");
    assert_eq!(rows(&panel, cx).len(), 2);
    cx.simulate_keystrokes("down enter");
    let expanded = panel.read_with(cx, |panel, _| panel.rows()[1].expanded);
    assert!(!expanded);
    cx.simulate_keystrokes("shift-down");
    assert_eq!(
        panel.read_with(cx, |panel, _| panel.selected.clone()),
        Some(Some(vec![0]))
    );
}

#[gpui_kit::test]
fn workspaces_reopen_and_failures_show(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("r.workspace");
    fs::write(
        &path,
        r#"<NotepadPlus><Project name="Kept"><File name="x.txt"/></Project></NotepadPlus>"#,
    )
    .unwrap();
    let (workspace, cx) = open_workspace(cx);
    cx.update(|_, cx| {
        AppState::update_state(cx, |state, _| {
            state.panels.projects.insert("3".into(), path.clone());
        });
    });
    let panel = panel(&workspace, 3, cx);
    assert_eq!(
        rows(&panel, cx),
        ["r.workspace", "  Kept", "    x.txt (missing)"]
    );

    // Changed on disk: Reload reads it again.
    fs::write(
        &path,
        r#"<NotepadPlus><Project name="Changed"/></NotepadPlus>"#,
    )
    .unwrap();
    panel.update(cx, |panel, cx| panel.reload(cx));
    assert_eq!(rows(&panel, cx), ["r.workspace", "  Changed"]);

    // A file that is no workspace: an empty one, and why.
    fs::write(&path, "<Session/>").unwrap();
    panel.update(cx, |panel, cx| panel.reload(cx));
    let error = panel.read_with(cx, |panel, _| panel.error.clone());
    assert!(error.is_some_and(|error| error.contains("not a Notepad++ workspace")));
    assert_eq!(rows(&panel, cx), ["r.workspace"]);

    // Saving where it cannot: the error shows and nothing is lost.
    let nowhere = dir.path().join("no").join("such.workspace");
    panel.update(cx, |panel, cx| {
        panel.open_workspace(nowhere.clone(), cx);
        panel.add_project("Unsaved", cx);
    });
    let (error, projects) = panel.read_with(cx, |panel, _| {
        (panel.error.clone(), panel.model.projects.len())
    });
    assert!(
        error.is_some_and(|error| error.starts_with("cannot write")),
        "the save failed"
    );
    assert_eq!(projects, 1);

    // Save As moves the panel to the new file; Save a Copy As does not.
    let copy = dir.path().join("copy.workspace");
    let moved = dir.path().join("moved.workspace");
    panel.update(cx, |panel, cx| {
        panel.save_as(copy.clone(), false, cx);
        panel.save_as(moved.clone(), true, cx);
    });
    assert!(copy.is_file() && moved.is_file());
    assert_eq!(
        panel.read_with(cx, |panel, _| panel.file.clone()),
        Some(moved.clone())
    );
    let remembered = cx.update(|_, cx| AppState::global(cx).state.panels.projects.clone());
    assert_eq!(remembered.get("3"), Some(&moved));
}

/// Right-clicks row `index` and picks the menu's `item`-th entry (from 1) with the keyboard.
fn pick(index: usize, item: usize, cx: &mut VisualTestContext) {
    let selector: &'static str = format!("project-row-{index}").leak();
    let center = cx.debug_bounds(selector).expect("drawn").center();
    cx.simulate_mouse_down(center, gpui_kit::MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(center, gpui_kit::MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    let keys = vec!["down"; item].join(" ");
    cx.simulate_keystrokes(&format!("{keys} enter"));
    cx.run_until_parked();
}

/// Answers the open name dialog with `name` (`None`: Escape).
fn answer(name: Option<&str>, cx: &mut VisualTestContext) {
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    match name {
        Some(name) => {
            cx.simulate_keystrokes(&secondary("a"));
            cx.simulate_input(name);
            cx.simulate_keystrokes("enter");
        }
        None => cx.simulate_keystrokes("escape"),
    }
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_buttons_and_right_click_menus_build_a_workspace(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("built.workspace");
    let [a, b] = [dir.path().join("a.txt"), dir.path().join("b.txt")];
    fs::write(&a, "a").unwrap();
    fs::write(&b, "b").unwrap();
    fs::create_dir(dir.path().join("lib")).unwrap();
    fs::write(dir.path().join("lib").join("c.rs"), "c").unwrap();
    let (workspace, cx) = open_workspace(cx);
    let panel = panel(&workspace, 1, cx);

    // No workspace yet: its button asks where to keep one; a cancelled question makes none.
    let new = cx.debug_bounds("project-new").expect("drawn").center();
    cx.simulate_click(new, Modifiers::none());
    cx.simulate_new_path_selection(|_| None);
    cx.run_until_parked();
    assert!(rows(&panel, cx).is_empty());
    cx.simulate_click(new, Modifiers::none());
    let chosen = path.clone();
    cx.simulate_new_path_selection(move |_| Some(chosen));
    cx.run_until_parked();
    assert_eq!(rows(&panel, cx), ["built.workspace"]);

    // The workspace's menu: Add New Project..., cancelled, then named.
    pick(0, 6, cx);
    answer(None, cx);
    assert_eq!(rows(&panel, cx), ["built.workspace"]);
    pick(0, 6, cx);
    answer(Some("Editor"), cx);
    assert_eq!(rows(&panel, cx), ["built.workspace", "  Editor"]);

    // A project's menu: Rename..., Add Folder..., Add Files..., Add Files from Directory....
    pick(1, 1, cx);
    answer(Some("Core"), cx);
    pick(1, 2, cx);
    answer(Some("Docs"), cx);
    pick(1, 3, cx);
    let files = vec![a.clone(), b.clone()];
    cx.simulate_path_prompt_response(move |_| Some(files));
    cx.run_until_parked();
    pick(1, 4, cx);
    let lib = dir.path().join("lib");
    cx.simulate_path_prompt_response(move |_| Some(vec![lib]));
    cx.run_until_parked();
    assert_eq!(
        rows(&panel, cx),
        [
            "built.workspace",
            "  Core",
            "    Docs",
            "    a.txt",
            "    b.txt",
            "    c.rs"
        ]
    );

    // A file's menu: Move Up, Move Down (nothing below the last), Remove, Open.
    pick(4, 4, cx);
    assert_eq!(rows(&panel, cx)[3..5], ["    b.txt", "    a.txt"]);
    pick(5, 5, cx);
    assert_eq!(rows(&panel, cx)[5], "    c.rs", "the last item stays last");
    pick(5, 3, cx);
    assert_eq!(rows(&panel, cx).len(), 5, "c.rs removed");
    pick(4, 1, cx);
    assert_eq!(tab_names(&workspace, cx), ["a.txt"]);
    // A folder's menu removes it.
    pick(2, 5, cx);
    assert_eq!(
        rows(&panel, cx),
        ["built.workspace", "  Core", "    b.txt", "    a.txt"]
    );

    // Every change is saved at once; Save a Copy As... leaves the panel on its file.
    let saved = ProjectWorkspace::load(&path).unwrap();
    assert_eq!(saved, panel.read_with(cx, |panel, _| panel.model.clone()));
    let copy = dir.path().join("copy.workspace");
    pick(0, 5, cx);
    let chosen = copy.clone();
    cx.simulate_new_path_selection(move |_| Some(chosen));
    cx.run_until_parked();
    assert_eq!(ProjectWorkspace::load(&copy).unwrap(), saved);
    assert_eq!(
        panel.read_with(cx, |panel, _| panel.file.clone()),
        Some(path)
    );
}
