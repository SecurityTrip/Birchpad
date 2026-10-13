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
