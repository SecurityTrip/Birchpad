use std::collections::HashSet;

use gpui_kit::{Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};

use super::document_list::Column;
use super::*;
use crate::workspace::tests::{open_workspace, secondary, tab_names};

#[test]
fn every_panel_has_an_id_a_title_a_side_and_a_command() {
    let mut ids = HashSet::new();
    let mut titles = HashSet::new();
    for kind in PanelKind::ALL {
        assert_eq!(PanelKind::from_id(kind.id()), Some(kind));
        assert_eq!(PanelKind::of_invocation(&kind.invocation()), Some(kind));
        assert!(ids.insert(kind.id()), "{kind:?}");
        assert!(titles.insert(kind.title()), "{kind:?}");
    }
    assert_eq!(PanelKind::FunctionList.side(), Side::Right);
    assert_eq!(PanelKind::DocumentMap.side(), Side::Right);
    assert_eq!(PanelKind::Project(2).side(), Side::Left);
    // Names no panel has, and commands that are no panel's.
    for id in ["", "project-0", "project-4", "Function-List"] {
        assert_eq!(PanelKind::from_id(id), None, "{id:?}");
    }
    let project =
        |panel: u8| Invocation::with_args("panel.project", serde_json::json!({ "panel": panel }));
    assert_eq!(PanelKind::of_invocation(&project(4)), None);
    assert_eq!(
        PanelKind::of_invocation(&Invocation::new("panel.project")),
        None
    );
    assert_eq!(PanelKind::of_invocation(&Invocation::new("file.new")), None);
    // The project panels are 1 to 3.
    assert!(project_panel(0).is_err());
    assert_eq!(project_panel(1).unwrap(), PanelKind::Project(1));
    assert_eq!(project_panel(3).unwrap(), PanelKind::Project(3));
    assert!(project_panel(4).is_err());
}

fn run(workspace: &Entity<Workspace>, invocation: Invocation, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&invocation, window, cx).unwrap();
    });
    cx.run_until_parked();
}

fn docks(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> (Vec<PanelKind>, Option<PanelKind>, Option<PanelKind>) {
    workspace.read_with(cx, |workspace, _| {
        (
            workspace.docks.open.clone(),
            workspace.docks.shown(Side::Left),
            workspace.docks.shown(Side::Right),
        )
    })
}

fn remembered(cx: &mut VisualTestContext) -> Vec<String> {
    cx.update(|_, cx| AppState::global(cx).state.panels.open.clone())
}

#[gpui_kit::test]
fn panels_open_close_and_share_their_dock(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    assert_eq!(docks(&workspace, cx), (vec![], None, None));

    run(&workspace, PanelKind::DocumentList.invocation(), cx);
    run(&workspace, PanelKind::FunctionList.invocation(), cx);
    run(&workspace, PanelKind::Project(2).invocation(), cx);
    assert_eq!(
        docks(&workspace, cx),
        (
            vec![
                PanelKind::DocumentList,
                PanelKind::FunctionList,
                PanelKind::Project(2)
            ],
            Some(PanelKind::Project(2)),
            Some(PanelKind::FunctionList)
        ),
        "the last opened on each side is shown"
    );
    assert_eq!(
        remembered(cx),
        ["document-list", "function-list", "project-2"]
    );
    let checked = workspace.read_with(cx, |workspace, _| {
        PanelKind::ALL.map(|kind| is_checked(&workspace.docks, &kind.invocation()))
    });
    assert_eq!(
        checked
            .iter()
            .filter(|checked| **checked == Some(true))
            .count(),
        3
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| is_checked(
            &workspace.docks,
            &Invocation::new("file.new")
        )),
        None
    );

    // A tab of the left dock.
    workspace.update(cx, |workspace, cx| {
        workspace.show_panel(PanelKind::DocumentList, cx);
    });
    assert_eq!(docks(&workspace, cx).1, Some(PanelKind::DocumentList));
    // Showing a panel that is not open does nothing.
    workspace.update(cx, |workspace, cx| {
        workspace.show_panel(PanelKind::DocumentMap, cx)
    });
    assert_eq!(docks(&workspace, cx).2, Some(PanelKind::FunctionList));

    // Closing the shown panel shows the one opened last on that side; the menu command closes
    // an open panel.
    workspace.update(cx, |workspace, cx| {
        workspace.close_panel(PanelKind::DocumentList, cx);
    });
    assert_eq!(docks(&workspace, cx).1, Some(PanelKind::Project(2)));
    run(&workspace, PanelKind::Project(2).invocation(), cx);
    run(&workspace, PanelKind::FunctionList.invocation(), cx);
    assert_eq!(docks(&workspace, cx), (vec![], None, None));
    assert!(remembered(cx).is_empty());
    // Closing a panel that is not open changes nothing.
    workspace.update(cx, |workspace, cx| {
        workspace.close_panel(PanelKind::DocumentMap, cx);
    });
    assert_eq!(docks(&workspace, cx), (vec![], None, None));
}

#[gpui_kit::test]
fn the_panels_open_at_the_last_quit_come_back(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.update(|_, cx| {
        AppState::update_state(cx, |state, _| {
            state.panels.open = vec![
                "function-list".into(),
                "no-such-panel".into(),
                "document-list".into(),
                "function-list".into(),
            ];
        });
    });
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.restore_panels(window, cx)
    });
    assert_eq!(
        docks(&workspace, cx),
        (
            vec![PanelKind::FunctionList, PanelKind::DocumentList],
            Some(PanelKind::DocumentList),
            Some(PanelKind::FunctionList)
        ),
        "unknown names are left out, a name twice opens it once"
    );
    assert_eq!(remembered(cx), ["function-list", "document-list"]);
}

fn document_list(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> Entity<DocumentList> {
    workspace.read_with(cx, |workspace, _| {
        workspace
            .docks
            .view(PanelKind::DocumentList)
            .expect("open")
            .clone()
            .downcast::<DocumentList>()
            .expect("a document list")
    })
}

fn rows(list: &Entity<DocumentList>, cx: &mut VisualTestContext) -> Vec<String> {
    list.read_with(cx, |list, cx| {
        list.entries(cx)
            .iter()
            .map(|entry| {
                let active = if entry.active { "> " } else { "" };
                format!("{active}{}", entry.label())
            })
            .collect()
    })
}

#[gpui_kit::test]
fn the_document_list_shows_clicks_and_closes_documents(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("changed");
    cx.simulate_keystrokes(&secondary("n"));
    cx.simulate_keystrokes(&secondary("n"));
    run(&workspace, PanelKind::DocumentList.invocation(), cx);
    let list = document_list(&workspace, cx);
    assert_eq!(rows(&list, cx), ["new 1 *", "new 2", "> new 3"]);

    // A click shows a document.
    let row = cx.debug_bounds("document-list-0").expect("drawn").center();
    cx.simulate_click(row, Modifiers::none());
    assert_eq!(rows(&list, cx), ["> new 1 *", "new 2", "new 3"]);

    // In split view, the second view's documents are marked.
    run(&workspace, Invocation::new("view.move-to-other-view"), cx);
    assert_eq!(rows(&list, cx), ["new 2", "new 3", "> new 1 * (2)"]);

    // A middle click closes; a modified document is asked about first.
    let row = cx.debug_bounds("document-list-1").expect("drawn").center();
    cx.simulate_mouse_down(row, MouseButton::Middle, Modifiers::none());
    cx.simulate_mouse_up(row, MouseButton::Middle, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx), ["new 2", "new 1"]);
    let row = cx.debug_bounds("document-list-1").expect("drawn").center();
    cx.simulate_mouse_down(row, MouseButton::Middle, Modifiers::none());
    cx.simulate_mouse_up(row, MouseButton::Middle, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert_eq!(tab_names(&workspace, cx), ["new 2", "new 1"], "kept");
}

#[gpui_kit::test]
fn the_document_list_sorts_by_a_column_and_back(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (workspace, cx) = open_workspace(cx);
    for name in ["b.txt", "A.txt", "c.txt"] {
        let path = dir.path().join(name);
        std::fs::write(&path, "").unwrap();
        workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_path(&path, window, cx)
        });
        cx.run_until_parked();
    }
    cx.simulate_keystrokes(&secondary("n"));
    run(&workspace, PanelKind::DocumentList.invocation(), cx);
    let list = document_list(&workspace, cx);
    let names = |cx: &mut VisualTestContext| {
        list.read_with(cx, |list, cx| {
            list.entries(cx)
                .into_iter()
                .map(|entry| entry.name)
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(
        names(cx),
        ["b.txt", "A.txt", "c.txt", "new 1"],
        "the tabs' order"
    );
    let heading = cx
        .debug_bounds("document-list-heading-Name")
        .expect("drawn")
        .center();
    cx.simulate_click(heading, Modifiers::none());
    assert_eq!(
        names(cx),
        ["A.txt", "b.txt", "c.txt", "new 1"],
        "without regard to case"
    );
    cx.simulate_click(heading, Modifiers::none());
    assert_eq!(names(cx), ["new 1", "c.txt", "b.txt", "A.txt"]);
    cx.simulate_click(heading, Modifiers::none());
    assert_eq!(names(cx), ["b.txt", "A.txt", "c.txt", "new 1"]);
    // By path, an untitled document (no path) first.
    list.update(cx, |list, cx| list.sort_by(Column::Path, cx));
    assert_eq!(names(cx), ["new 1", "A.txt", "b.txt", "c.txt"]);
    // Another column starts ascending again.
    list.update(cx, |list, cx| {
        list.sort_by(Column::Path, cx);
        list.sort_by(Column::Name, cx);
    });
    assert_eq!(
        list.read_with(cx, |list, _| list.sort),
        Some((Column::Name, false))
    );
}
