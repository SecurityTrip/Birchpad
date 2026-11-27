use birchpad::app_state::AppState;
use gpui_kit::TestAppContext;

use crate::workspace::{active_text, open_workspace};

#[gpui_kit::test]
fn vim_is_opt_in_and_switches_between_normal_insert_and_visual(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.simulate_keystrokes("i");
    assert_eq!(active_text(&workspace, cx), "i");
    cx.update(|_, cx| cx.global_mut::<AppState>().settings.editor.vim_mode = true);

    cx.simulate_keystrokes("0 z i a b c escape 0 v l d");
    assert_eq!(active_text(&workspace, cx), "ci");
    cx.simulate_keystrokes("u");
    assert_eq!(active_text(&workspace, cx), "abci");
    cx.simulate_keystrokes("ctrl-r");
    assert_eq!(active_text(&workspace, cx), "ci");
}

#[gpui_kit::test]
fn vim_line_operators_obey_read_only_and_handle_the_end_of_the_document(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.update(|_, cx| cx.global_mut::<AppState>().settings.editor.vim_mode = true);
    cx.simulate_keystrokes("d d x i a enter b escape g g");
    assert_eq!(active_text(&workspace, cx).replace("\r\n", "\n"), "a\nb");

    let buffer = workspace.read_with(cx, |workspace, cx| {
        workspace.active_view(cx).unwrap().read(cx).buffer.clone()
    });
    buffer.update(cx, |buffer, cx| buffer.set_requested_read_only(true, cx));
    cx.simulate_keystrokes("d d i x");
    assert_eq!(active_text(&workspace, cx).replace("\r\n", "\n"), "a\nb");
    buffer.update(cx, |buffer, cx| buffer.set_requested_read_only(false, cx));
    cx.simulate_keystrokes("shift-g d d");
    assert_eq!(active_text(&workspace, cx), "a");
    cx.simulate_keystrokes("u");
    assert_eq!(active_text(&workspace, cx).replace("\r\n", "\n"), "a\nb");
}
