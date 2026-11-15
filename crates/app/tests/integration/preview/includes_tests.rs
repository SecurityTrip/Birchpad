//! Tests of the includes the preview follows (ADR 0062): paths written with the preprocessor's
//! variables and functions worked out as PlantUML works them out.

use std::path::{Path, PathBuf};

use gpui_kit::TestAppContext;

use crate::workspace::open_workspace;

/// The repository's layout: a document under `views`, a library under `repository`, both
/// reached through `$DIAGRAMS_ROOT` and a function of its own.
fn repository() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let views = root.join("views").join("context");
    let services = root.join("library").join("services");
    let billing = root.join("parts").join("billing");
    for folder in [&views, &services, &billing] {
        std::fs::create_dir_all(folder).unwrap();
    }
    std::fs::write(
        root.join("library").join("main.iuml"),
        "!function FromRoot($p)\n!return $DIAGRAMS_ROOT + $p\n!endfunction\n\
         !include $DIAGRAMS_ROOT/library/services/all.iuml\n",
    )
    .unwrap();
    std::fs::write(services.join("all.iuml"), "class Service\n").unwrap();
    std::fs::write(billing.join("containers.puml"), "class Billing\n").unwrap();
    (dir, views)
}

const DOCUMENT: &str = "@startuml\n!$DIAGRAMS_ROOT ?= \"../..\"\n\
                        !include $DIAGRAMS_ROOT/library/main.iuml\n\
                        !include FromRoot(/parts/billing/containers.puml)\n@enduml\n";

#[gpui_kit::test]
fn ctrl_click_on_an_include_through_a_function_opens_its_file(cx: &mut TestAppContext) {
    let (workspace, cx) = open_workspace(cx);
    let (dir, views) = repository();
    let document = views.join("context.puml");
    std::fs::write(&document, DOCUMENT).unwrap();
    let invocation = birchpad_commands::Invocation::with_args(
        "preview.open-include",
        serde_json::json!({
            "target": "FromRoot(/parts/billing/containers.puml)",
            "document": document,
        }),
    );
    let opened = workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&invocation, window, cx)
    });
    cx.run_until_parked();
    opened.unwrap();
    let active = workspace.read_with(cx, |workspace, cx| {
        let view = workspace.active_view(cx).unwrap();
        view.read(cx).buffer.read(cx).path().map(Path::to_path_buf)
    });
    assert_eq!(
        active,
        Some(
            dir.path()
                .join("parts")
                .join("billing")
                .join("containers.puml")
        )
    );
}
