//! Right-to-left rows in the editor (ADR 0035): `birchpad_view::BidiRow` in a window.

fn editor(
    cx: &mut gpui_kit::TestAppContext,
) -> (
    gpui_kit::Entity<crate::test_support::Workspace>,
    &mut gpui_kit::VisualTestContext,
) {
    crate::test_support::open_workspace(cx)
}

#[gpui_kit::test]
fn hebrew_rows_show_turned_and_carets_follow(cx: &mut gpui_kit::TestAppContext) {
    let (workspace, cx) = editor(cx);
    cx.simulate_input("אבג");
    cx.run_until_parked();
    let view = workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap());
    view.read_with(cx, |view, _| {
        let layout = view.layout.as_ref().expect("a frame was drawn");
        let row = &layout.rows[0];
        assert_eq!(
            row.bidi.as_ref().map(|bidi| bidi.visual.as_str()),
            Some("גבא")
        );
        // The start of the word shows right of its end.
        assert!(row.x_for(0).unwrap() > row.x_for(6).unwrap());
    });
    // Clicking at the left end puts the caret after the word.
    cx.simulate_keystrokes("home");
    let at_left = view.read_with(cx, |view, _| {
        let layout = view.layout.as_ref().unwrap();
        let row = &layout.rows[0];
        gpui_kit::point(
            row.x + gpui_kit::px(1.),
            row.y + layout.metrics.line_height / 2.,
        )
    });
    cx.simulate_click(at_left, gpui_kit::Modifiers::none());
    assert_eq!(
        view.read_with(cx, |view, _| view.selection.primary().head),
        6
    );
}

#[gpui_kit::test]
fn right_to_left_rows_end_at_the_right_edge(cx: &mut gpui_kit::TestAppContext) {
    use birchpad_commands::Invocation;
    let (workspace, cx) = editor(cx);
    cx.simulate_input("ab");
    let view = workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap());
    let left = view.read_with(cx, |view, _| view.layout.as_ref().unwrap().rows[0].x);
    workspace.update_in(cx, |workspace, window, cx| {
        workspace
            .dispatch(&Invocation::new("edit.text-direction-rtl"), window, cx)
            .unwrap();
    });
    cx.run_until_parked();
    let (right, edge) = view.read_with(cx, |view, _| {
        let layout = view.layout.as_ref().unwrap();
        let row = &layout.rows[0];
        (row.x + row.shaped.width(), layout.text_bounds.right())
    });
    assert!(
        right > left + gpui_kit::px(100.) && right <= edge,
        "{right:?} {edge:?}"
    );
    workspace.update_in(cx, |workspace, window, cx| {
        workspace
            .dispatch(&Invocation::new("edit.text-direction-ltr"), window, cx)
            .unwrap();
    });
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.layout.as_ref().unwrap().rows[0].x),
        left
    );
}

#[gpui_kit::test]
fn the_margins_move_right_and_keys_switch_the_direction(cx: &mut gpui_kit::TestAppContext) {
    let (workspace, cx) = editor(cx);
    cx.simulate_input("ab");
    let view = workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap());
    let sides = |cx: &mut gpui_kit::VisualTestContext| {
        view.read_with(cx, |view, _| {
            let layout = view.layout.as_ref().unwrap();
            (layout.margins.gutter, layout.text_bounds)
        })
    };
    let (gutter, text) = sides(cx);
    assert!(
        gutter.right() <= text.left(),
        "left to right: margins at the left"
    );
    cx.simulate_keystrokes("ctrl-alt-shift-r");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.rtl));
    let (gutter, text) = sides(cx);
    assert!(
        text.right() <= gutter.left(),
        "right to left: margins at the right"
    );
    assert!(gutter.size.width > gpui_kit::px(0.));
    cx.simulate_keystrokes("ctrl-alt-shift-l");
    cx.run_until_parked();
    assert!(!view.read_with(cx, |view, _| view.rtl));
    // The direction goes with the view's state, as sessions keep it.
    let state = view.read_with(cx, |view, cx| view.view_state(cx));
    assert!(!state.rtl);
    view.update(cx, |view, cx| {
        view.restore(crate::ViewState { rtl: true, ..state }, cx);
    });
    assert!(view.read_with(cx, |view, cx| view.view_state(cx).rtl));
}

#[gpui_kit::test]
fn a_selection_over_mixed_text_is_drawn_in_pieces(cx: &mut gpui_kit::TestAppContext) {
    let (workspace, cx) = editor(cx);
    // "ab אב cd": selecting from "b" into the Hebrew word covers two pieces apart.
    cx.simulate_input("ab \u{5d0}\u{5d1} cd");
    let view = workspace.read_with(cx, |workspace, cx| workspace.active_view(cx).unwrap());
    let hebrew = "ab ".len();
    view.update(cx, |view, cx| view.select_range(1..hebrew + 2, cx));
    cx.run_until_parked();
    let pieces = view.read_with(cx, |view, _| view.layout.as_ref().unwrap().selections.len());
    assert_eq!(pieces, 2);
    // Within one direction, one piece; nothing selected, none.
    view.update(cx, |view, cx| view.select_range(0..2, cx));
    cx.run_until_parked();
    let pieces = view.read_with(cx, |view, _| view.layout.as_ref().unwrap().selections.len());
    assert_eq!(pieces, 1);
    view.update(cx, |view, cx| view.select_range(2..2, cx));
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| {
        view.layout.as_ref().unwrap().selections.is_empty()
    }));
}
