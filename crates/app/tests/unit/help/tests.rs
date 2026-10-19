use birchpad_commands::Invocation;
use birchpad_config::UpdateMode;
use gpui_kit::TestAppContext;

use super::*;
use crate::workspace::tests::open_workspace;

#[gpui_kit::test]
fn about_lists_build_paths_and_policies(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().policies =
            vec![("updates.mode".to_owned(), "\"off\"".to_owned())];
        cx.global_mut::<AppState>().settings.updates.mode = UpdateMode::Off;
    });
    let text = cx.update(|_, cx| about_text(cx));
    assert!(
        text.starts_with(&format!("Birchpad\nVersion:  {VERSION}\nBuild:    ")),
        "{text}"
    );
    assert!(
        text.contains("\nInstalled:not by Birchpad's installer\n"),
        "{text}"
    );
    assert!(text.contains("\nUpdates:  turned off\n"), "{text}");
    assert!(
        text.ends_with("\nPolicies: updates.mode = \"off\""),
        "{text}"
    );

    workspace.update_in(cx, |workspace, window, cx| {
        workspace
            .dispatch(&Invocation::new("help.about"), window, cx)
            .unwrap();
    });
    cx.run_until_parked();
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
}
