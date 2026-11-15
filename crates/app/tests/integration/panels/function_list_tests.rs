use birchpad_commands::Invocation;
use gpui_kit::{Modifiers, TestAppContext, VisualTestContext};
use serde_json::json;

use crate::workspace::open_workspace;
use birchpad::panels::PanelKind;
use birchpad::panels::function_list::*;
use birchpad::workspace::Workspace;
use gpui_kit::Entity;

// --- In the window -------------------------------------------------------------------------

fn run(workspace: &Entity<Workspace>, invocation: Invocation, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&invocation, window, cx).unwrap();
    });
    cx.run_until_parked();
}

fn panel(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<FunctionList> {
    workspace.read_with(cx, |workspace, _| {
        workspace
            .docks
            .view(PanelKind::FunctionList)
            .expect("open")
            .clone()
            .downcast::<FunctionList>()
            .expect("a function list")
    })
}

/// What the panel shows: the rows (the current one marked) or its message.
fn shown(list: &Entity<FunctionList>, cx: &mut VisualTestContext) -> Vec<String> {
    list.update(cx, |list, cx| match list.contents(cx) {
        Contents::Message(message) => vec![message],
        Contents::Tree { rows, current, .. } => rows
            .iter()
            .map(|row| {
                let symbol = list.symbol(row.symbol).unwrap();
                let mark = if Some(row.symbol) == current {
                    ">"
                } else {
                    " "
                };
                format!("{mark}{}{}", "  ".repeat(row.depth), symbol.name)
            })
            .collect(),
    })
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
fn the_function_list_follows_the_document(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    run(&workspace, PanelKind::FunctionList.invocation(), cx);
    let list = panel(&workspace, cx);
    assert_eq!(shown(&list, cx), ["Plain text has no function list"]);
    run(
        &workspace,
        Invocation::with_args("language.set", json!({ "language": "json" })),
        cx,
    );
    assert_eq!(shown(&list, cx), ["No function list for JSON"]);

    run(
        &workspace,
        Invocation::with_args("language.set", json!({ "language": "rust" })),
        cx,
    );
    cx.simulate_input("struct S;\nimpl S {\n    fn one() {}\n}\nfn two() {}");
    cx.run_until_parked();
    assert_eq!(
        shown(&list, cx),
        [" S", " S", "   one", " two"],
        "the caret is past the end of two"
    );
    cx.simulate_keystrokes("left");
    assert_eq!(shown(&list, cx)[3], ">two");

    // A click goes to the definition and selects its name, also right of the name.
    let row = cx.debug_bounds("function-list-2").expect("drawn");
    let right = gpui_kit::point(row.right() - gpui_kit::px(4.), row.center().y);
    cx.simulate_click(right, Modifiers::none());
    assert_eq!(selection(&workspace, cx), (26, 29));
    assert_eq!(shown(&list, cx)[2], ">  one");

    // New code shows once the tree has it.
    workspace.update(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.update(cx, |view, cx| {
            let end = view.text(cx).len();
            view.go_to(end, cx);
        });
    });
    cx.simulate_input("\nfn three() {}");
    cx.run_until_parked();
    assert_eq!(shown(&list, cx).last().map(String::as_str), Some(" three"));
}

#[gpui_kit::test]
fn folding_sorting_and_filtering_in_the_panel(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    run(
        &workspace,
        Invocation::with_args("language.set", json!({ "language": "python" })),
        cx,
    );
    cx.simulate_input("def zed():\n    pass\n\nclass Box:\n    def put(self):\n        pass\n");
    run(&workspace, PanelKind::FunctionList.invocation(), cx);
    let list = panel(&workspace, cx);
    assert_eq!(shown(&list, cx), [" zed", " Box", "   put"]);

    let fold = cx
        .debug_bounds("function-list-fold-1")
        .expect("drawn")
        .center();
    cx.simulate_click(fold, Modifiers::none());
    assert_eq!(
        shown(&list, cx),
        [" zed", " Box"],
        "folded, without going there"
    );
    assert_eq!(selection(&workspace, cx).0, selection(&workspace, cx).1);
    // Sorted by name, still folded.
    list.update(cx, |list, cx| {
        list.sorted = true;
        cx.notify();
    });
    assert_eq!(shown(&list, cx), [" Box", " zed"]);

    workspace.update_in(cx, |_, window, cx| {
        list.update(cx, |list, cx| list.set_filter("PU", window, cx));
    });
    assert_eq!(
        shown(&list, cx),
        [" Box", "   put"],
        "the filter opens what it finds"
    );
    workspace.update_in(cx, |_, window, cx| {
        list.update(cx, |list, cx| list.set_filter("nothing", window, cx));
    });
    assert!(shown(&list, cx).is_empty());
}
