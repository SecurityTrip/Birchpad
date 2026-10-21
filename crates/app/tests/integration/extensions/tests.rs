use birchpad_commands::Invocation;
use gpui_kit::{TestAppContext, VisualTestContext};
use serde_json::json;

use crate::workspace::{active_text, open_workspace};
use anyhow::Result;
use birchpad::app_state::AppState;
use birchpad::extensions::*;
use birchpad::panels::PanelKind;
use birchpad::panels::PluginPanel;
use birchpad::workspace::Workspace;
use gpui_kit::Entity;

fn setup(
    cx: &mut TestAppContext,
) -> (Entity<Workspace>, &mut VisualTestContext, tempfile::TempDir) {
    let (workspace, cx) = open_workspace(cx);
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("scripts")).unwrap();
    cx.update(|_, cx| {
        cx.global_mut::<AppState>().paths.user_settings = Some(dir.path().join("settings.toml"));
    });
    (workspace, cx, dir)
}

fn script(dir: &tempfile::TempDir, name: &str, source: &str) {
    std::fs::write(
        dir.path().join("scripts").join(format!("{name}.rhai")),
        source,
    )
    .unwrap();
}

fn run(
    workspace: &Entity<Workspace>,
    invocation: Invocation,
    cx: &mut VisualTestContext,
) -> Result<()> {
    let result = workspace.update_in(cx, |workspace, window, cx| {
        workspace.dispatch(&invocation, window, cx)
    });
    cx.run_until_parked();
    result
}

fn run_script(workspace: &Entity<Workspace>, name: &str, cx: &mut VisualTestContext) -> Result<()> {
    run(
        workspace,
        Invocation::with_args("script.run", json!({ "name": name })),
        cx,
    )
}

fn panel(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> Option<(String, Vec<String>)> {
    workspace.read_with(cx, |workspace, cx| {
        let panel = workspace
            .docks
            .view(PanelKind::PluginPanel)?
            .clone()
            .downcast::<PluginPanel>()
            .ok()?;
        let panel = panel.read(cx);
        Some((panel.title.clone(), panel.lines.clone()))
    })
}

#[gpui_kit::test]
fn runs_a_script_over_the_document_in_one_undo_step(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    script(&dir, "upper", "set_text(text().to_upper()); insert(\"!\");");
    script(&dir, "Another", "");
    assert_eq!(cx.update(|_, cx| scripts::names(cx)), ["Another", "upper"]);
    run(
        &workspace,
        Invocation::with_args("edit.insert-text", json!({ "text": "hello" })),
        cx,
    )
    .unwrap();
    run_script(&workspace, "upper", cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "HELLO!");
    run(&workspace, Invocation::new("edit.undo"), cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "hello");
}

#[gpui_kit::test]
fn runs_commands_and_fills_the_panel(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    script(
        &dir,
        "lines",
        r#"run_command("edit.insert-text", #{ text: "a\nb" });
           message("done");
           show_panel("Lines", ["one", "two"]);"#,
    );
    run_script(&workspace, "lines", cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "a\nb");
    assert_eq!(
        panel(&workspace, cx),
        Some(("Lines".to_owned(), vec!["one".to_owned(), "two".to_owned()]))
    );
    assert!(workspace.read_with(cx, |workspace, _| {
        workspace.docks.is_open(PanelKind::PluginPanel)
    }));
    // A click on a script's line does nothing.
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.plugin_panel_clicked(&PanelOwner::Script("lines".into()), 1, window, cx)
        })
        .unwrap();
}

#[gpui_kit::test]
fn a_failing_script_changes_nothing(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    script(&dir, "half", "set_text(\"changed\"); throw \"stop\";");
    script(&dir, "spin", "loop {}");
    script(
        &dir,
        "bad-command",
        "insert(\"x\"); run_command(\"edit.convert\", #{ to: \"rot13\" });",
    );
    let error = run_script(&workspace, "half", cx).unwrap_err();
    assert!(error.to_string().contains("stop"), "{error}");
    let error = run_script(&workspace, "spin", cx).unwrap_err();
    assert!(error.to_string().contains("ran too long"), "{error}");
    assert_eq!(active_text(&workspace, cx), "");
    // The edit stands; the command that fails is the error.
    assert!(run_script(&workspace, "bad-command", cx).is_err());
    assert_eq!(active_text(&workspace, cx), "x");
}

#[gpui_kit::test]
fn refuses_missing_scripts_and_bad_names(cx: &mut TestAppContext) {
    let (workspace, cx, _dir) = setup(cx);
    let error = run_script(&workspace, "missing", cx).unwrap_err();
    assert!(
        error.to_string().contains("cannot read the script"),
        "{error}"
    );
    for name in ["", "../x", ".hidden", "a\\b"] {
        assert!(run_script(&workspace, name, cx).is_err(), "{name}");
    }
}

#[gpui_kit::test]
fn follows_the_policy(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    script(&dir, "ok", "insert(\"x\");");
    cx.update(|_, cx| cx.global_mut::<AppState>().settings.plugins.scripts = false);
    let error = run_script(&workspace, "ok", cx).unwrap_err();
    assert!(error.to_string().contains("administrator"), "{error}");
    assert_eq!(active_text(&workspace, cx), "");
}

#[gpui_kit::test]
fn stops_scripts_that_run_themselves(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    script(
        &dir,
        "again",
        r#"insert("x"); run_command("script.run", #{ name: "again" });"#,
    );
    let error = run_script(&workspace, "again", cx).unwrap_err();
    assert!(error.to_string().contains("deep"), "{error}");
    assert_eq!(active_text(&workspace, cx), "x".repeat(MAX_DEPTH));
    // The depth is back to nothing.
    script(&dir, "once", "insert(\"y\");");
    run_script(&workspace, "once", cx).unwrap();
}

#[gpui_kit::test]
fn sees_the_documents_language_and_file(cx: &mut TestAppContext) {
    let (workspace, cx, dir) = setup(cx);
    script(
        &dir,
        "describe",
        "insert(language() + \" \" + (path() == ()));",
    );
    run_script(&workspace, "describe", cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "text true");
    run(
        &workspace,
        Invocation::with_args("language.set", json!({ "language": "rust" })),
        cx,
    )
    .unwrap();
    run(&workspace, Invocation::new("edit.select-all"), cx).unwrap();
    run_script(&workspace, "describe", cx).unwrap();
    assert_eq!(active_text(&workspace, cx), "rust true");
}

#[gpui_kit::test]
fn an_edit_needs_a_document(cx: &mut TestAppContext) {
    let (workspace, cx, _dir) = setup(cx);
    workspace.update_in(cx, |workspace, window, cx| {
        let outcome = birchpad_plugin::Outcome {
            transaction: Some(birchpad_core::Transaction::new(
                birchpad_core::ChangeSet::identity(0),
            )),
            effects: Vec::new(),
        };
        let error = workspace
            .apply_outcome(None, PanelOwner::Script("x".into()), outcome, window, cx)
            .unwrap_err();
        assert!(error.to_string().contains("no document"));
    });
}
