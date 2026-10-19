use gpui_kit::TestAppContext;

use crate::workspace::tests::{active_text, open_workspace, tab_names};

/// The drawn menu bar's commands reach the workspace, and the editor that had the focus.
#[gpui_kit::test]
fn menu_bar_commands_reach_the_workspace(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_input("abc");
    cx.simulate_keystrokes("alt-f n");
    assert_eq!(tab_names(&workspace, cx), ["new 1", "new 2"], "File > New");

    // Edit > Undo, in the first document.
    cx.simulate_keystrokes("ctrl-tab alt-e enter");
    assert_eq!(active_text(&workspace, cx), "");
}
