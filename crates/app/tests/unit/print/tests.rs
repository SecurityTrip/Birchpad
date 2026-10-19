use super::*;

#[test]
fn the_page_to_print_is_named_after_the_document() {
    let path = print_path("new 1:<x>");
    assert!(path.ends_with("birchpad-print-new_1__x_.html"));
    assert!(
        print_path("").ends_with("birchpad-print-.html"),
        "an empty title"
    );
}

#[gpui_kit::test]
fn print_writes_the_page_with_the_highlights(cx: &mut gpui_kit::TestAppContext) {
    use birchpad_commands::Invocation;
    use serde_json::json;
    let (workspace, cx) = crate::workspace::tests::open_workspace(cx);
    let run = |invocation: Invocation, cx: &mut gpui_kit::VisualTestContext| {
        let result = workspace.update_in(cx, |workspace, window, cx| {
            workspace.dispatch(&invocation, window, cx)
        });
        cx.run_until_parked();
        result
    };
    run(
        Invocation::with_args("language.set", json!({ "language": "rust" })),
        cx,
    )
    .unwrap();
    run(
        Invocation::with_args("edit.insert-text", json!({ "text": "fn main() {}" })),
        cx,
    )
    .unwrap();
    // Let the syntax tree be parsed.
    for _ in 0..50 {
        cx.run_until_parked();
    }
    run(Invocation::new("file.print"), cx).unwrap();
    let page = std::fs::read_to_string(print_path("new 1")).unwrap();
    assert!(page.contains("window.print()"));
    assert!(page.contains("main"));
    let html = workspace.read_with(cx, |workspace, cx| {
        active_html(workspace, false, cx).unwrap().1
    });
    assert!(html.contains("<span style="), "{html}");
    std::fs::remove_file(print_path("new 1")).ok();
}
