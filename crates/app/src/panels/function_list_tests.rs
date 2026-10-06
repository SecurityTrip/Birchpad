use birchpad_commands::Invocation;
use gpui_kit::{Modifiers, TestAppContext, VisualTestContext};
use serde_json::json;

use super::*;
use crate::panels::PanelKind;
use crate::workspace::tests::open_workspace;

fn symbol(depth: usize, name: &str, range: Range<usize>) -> Symbol {
    Symbol {
        kind: SymbolKind::Function,
        name: name.into(),
        name_range: range.start..range.start + name.len(),
        range,
        depth,
    }
}

/// zeta { beta, alpha { inner } }, gamma
fn outline() -> Vec<Symbol> {
    vec![
        symbol(0, "zeta", 0..50),
        symbol(1, "beta", 5..10),
        symbol(1, "alpha", 10..40),
        symbol(2, "inner", 20..30),
        symbol(0, "gamma", 60..70),
    ]
}

fn names(symbols: &[Symbol], rows: &[Row]) -> Vec<String> {
    rows.iter()
        .map(|row| {
            let fold = match (row.has_children, row.collapsed) {
                (true, true) => "+",
                (true, false) => "-",
                _ => " ",
            };
            format!(
                "{}{fold}{}",
                "  ".repeat(row.depth),
                symbols[row.symbol].name
            )
        })
        .collect()
}

#[test]
fn the_tree_follows_the_text_or_the_names() {
    let symbols = outline();
    let none = HashSet::new();
    assert_eq!(
        names(&symbols, &rows(&symbols, false, &none, "")),
        ["-zeta", "   beta", "  -alpha", "     inner", " gamma"]
    );
    assert_eq!(
        names(&symbols, &rows(&symbols, true, &none, "")),
        [" gamma", "-zeta", "  -alpha", "     inner", "   beta"],
        "each level by name"
    );
    assert!(rows(&[], true, &none, "").is_empty());
}

#[test]
fn folded_symbols_hide_their_children() {
    let symbols = outline();
    let collapsed: HashSet<String> = ["zeta\u{1f}alpha".to_owned()].into();
    assert_eq!(
        names(&symbols, &rows(&symbols, false, &collapsed, "")),
        ["-zeta", "   beta", "  +alpha", " gamma"]
    );
    // A symbol without children is never shown folded, and an unknown path folds nothing.
    let leaf: HashSet<String> = ["gamma".to_owned(), "nowhere".to_owned()].into();
    assert_eq!(rows(&symbols, false, &leaf, "").len(), 5);
}

#[test]
fn a_filter_keeps_the_matches_and_their_parents() {
    let symbols = outline();
    let collapsed: HashSet<String> = ["zeta".to_owned()].into();
    assert_eq!(
        names(&symbols, &rows(&symbols, false, &collapsed, " INN ")),
        ["-zeta", "  -alpha", "     inner"],
        "without regard to case, inside a folded symbol too"
    );
    assert_eq!(
        names(&symbols, &rows(&symbols, false, &collapsed, "a")),
        ["-zeta", "   beta", "   alpha", " gamma"],
        "inner is filtered out, so alpha has nothing to fold"
    );
    assert!(rows(&symbols, false, &collapsed, "nothing").is_empty());
}

#[test]
fn the_current_symbol_is_the_innermost_around_the_caret() {
    let symbols = outline();
    assert_eq!(current_symbol(&symbols, 0), Some(0), "the first byte");
    assert_eq!(current_symbol(&symbols, 25), Some(3));
    assert_eq!(current_symbol(&symbols, 12), Some(2));
    assert_eq!(current_symbol(&symbols, 40), Some(0), "just past alpha");
    assert_eq!(current_symbol(&symbols, 50), None, "just past zeta");
    assert_eq!(current_symbol(&symbols, 55), None);
    assert_eq!(current_symbol(&[], 0), None);
}

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
